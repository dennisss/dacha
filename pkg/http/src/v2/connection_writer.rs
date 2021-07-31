use std::sync::Arc;

use common::errors::*;
use common::io::{Writeable, Readable};
use common::task::ChildTask;

use crate::v2::types::*;
use crate::v2::headers::*;
use crate::v2::connection_state::*;
use crate::hpack;
use crate::proto::v2::*;
use crate::v2::frame_utils;
use crate::v2::connection_shared::*;
use crate::v2::body::{encode_request_body_v2, encode_response_body_v2};

/*
    Streams will be one in one of a few odd temporary states:
    - New stream created by  Client: pending 
        => the client task can block on getting th result.
            We mainly need to later on send it back a 
    - New stream received by Server: pending getting a response
        => We'll create a new task to generate the response.
        => later that initial task will end and instead become the sending task
*/


pub struct ConnectionWriter {
    shared: Arc<ConnectionShared>
}

impl ConnectionWriter {
    pub fn new(shared: Arc<ConnectionShared>) -> Self {
        Self { shared }
    }

    pub async fn run(
        self, mut writer: Box<dyn Writeable>, upgrade_payload: Option<Box<dyn Readable>>
    ) -> Result<()> {
        if let Some(mut reader) = upgrade_payload {
            let mut buf = [0u8; 4096];
            loop {
                let n = reader.read(&mut buf).await?;
                if n == 0 {
                    break;
                }
                writer.write_all(&buf[0..n]).await?;
            }
        }

        if !self.shared.is_server {
            writer.write_all(CONNECTION_PREFACE).await?;
        }

        let mut remote_settings_known;

        let connection_event_receiver;

        {
            let mut state = self.shared.state.lock().await;

            connection_event_receiver = state.connection_event_receiver.take()
                .ok_or(err_msg("Multiple ConnectionWriters started?"))?;

            let mut payload = vec![];
            state.local_pending_settings.serialize_payload(&state.local_settings, &mut payload);

            state.local_settings_ack_waiter = Some(ChildTask::spawn(
                Self::wait_for_settings_ack(self.shared.clone())));
            remote_settings_known = state.remote_settings_known;
            drop(state);

            // Write out the initial settings frame.
            let mut frame = vec![];
            FrameHeader { length: payload.len() as u32, typ: FrameType::SETTINGS, flags: 0, reserved: 0, stream_id: 0 }
                .serialize(&mut frame).unwrap();
            frame.extend(payload);
            writer.write_all(&frame).await?;
        }

        // Used to encode locally created headers to be sent to the other endpoint.
        // NOTE: This is shared across all streams on the connection.
        // TODO: COnsiderate this with the above lock.
        let mut local_header_encoder = {
            let state = self.shared.state.lock().await;
            hpack::Encoder::new(
                state.remote_settings[SettingId::HEADER_TABLE_SIZE] as usize)
        };

        // List of events which we've deferred processing due to remote settings not being known yet.
        let mut pending_events: Vec<ConnectionEvent> = vec![];


        // TODO: Once we fail, we should consider cleaning out all remaining events in the channel and
        // make sure that any pending requests to be sent are properly rejected with the right error
        // code/message (indicating that are retryable and weren't really sent)
        /*
        TODO: The desired behavior is that if there are too many requests pending, we should queue
        the next request. We could have an opt-in solution to failing if queuing is required.
        - We should also be able to query the connection to check how many requests/streams are pending

        */

        loop {
            // TODO: If we are gracefully shutting down, stop waiting for events once all pending
            // streams have been closed.

            let event = {
                if remote_settings_known && !pending_events.is_empty() {
                    pending_events.remove(0)
                } else {
                    connection_event_receiver.recv().await?
                }
            };

            // Until we receive the first settings acknowledgement, we will queue all events.
            // TODO: Find a better solution than this.
            if !remote_settings_known {
                let allow = match event {
                    ConnectionEvent::AcknowledgeSettings { .. } => true,
                    ConnectionEvent::Closing { .. } => true,
                    ConnectionEvent::Goaway { .. } => true,
                    _ => false
                };

                if !allow {
                    pending_events.push(event);
                    continue;
                }
            }

            match event {
                // TODO: Instead alwas enqueue requests and always 
                ConnectionEvent::SendRequest => {
                    // TODO: If anything in here fails, we should report it to the requester rather than
                    // killing the whole thread.

                    let mut connection_state = self.shared.state.lock().await;

                    // Checking that we are able to send a stream.
                    let remote_stream_limit = std::cmp::min(
                        connection_state.remote_settings[SettingId::MAX_CONCURRENT_STREAMS],
                            self.shared.options.max_outgoing_streams as u32);
                    if connection_state.local_stream_count >= remote_stream_limit as usize {
                        continue;
                    }

                    let (mut request, response_handler) = match connection_state.pending_requests.pop_front() {
                        Some(v) => (v.request, v.response_handler),
                        None => continue
                    };

                    let body = encode_request_body_v2(&mut request.head, request.body);

                    // Generate a new stream id.
                    let stream_id = {
                        if connection_state.last_sent_stream_id == 0 {
                            if self.shared.is_server { 2 } else { 1 }
                        } else {
                            connection_state.last_sent_stream_id + 2
                        }
                    };
                    connection_state.last_sent_stream_id = stream_id;

                    let (mut stream, incoming_body, outgoing_body) = self.shared.new_stream(
                        &connection_state, stream_id);

                    // The type of request will determine if we allow a response

                    // Apply client request specific details to the stream's state. 
                    let local_end = {
                        stream.incoming_response_handler = Some((request.head.method, response_handler, incoming_body));

                        // TODO: Deduplicate this logic with the other side.
                        // TODO: Also ensure that the case of zero length bodies is optimized when
                        // it comes to intermediate body compression layers.
                        if let Some(body) = body {
                            // NOTE: Because we are still blocking the writing thread later down in this function,
                            // this won't trigger DATA frames to be sent until the HEADERS frame will be sent.
                            stream.processing_tasks.push(ChildTask::spawn(outgoing_body.run(body)));
                            false
                        } else {
                            // TODO: Lock and mark as locally closed?
                            let mut stream_state = stream.state.lock().await;
                            stream_state.sending_end = true;
                            stream.sending_end_flushed = true;

                            true
                        }
                    };

                    connection_state.local_stream_count += 1;
                    connection_state.streams.insert(stream_id, stream);

                    let max_remote_frame_size = connection_state.remote_settings[SettingId::MAX_FRAME_SIZE] as usize;

                    drop(connection_state);

                    // We are now done setting up the stream.
                    // Now we should just send the request to the other side.

                    // TODO: Split this up into the request validation and header encoding components so that we can
                    // return errors if it is invalid.
                    let header_block = encode_request_headers_block(
                        &request.head, &mut local_header_encoder)?;

                    write_headers_block(writer.as_mut(), stream_id, &header_block, local_end,
                                        max_remote_frame_size).await?;
                }
                ConnectionEvent::SendPushPromise { request, response } => {

                }
                ConnectionEvent::SendResponse { stream_id, mut response } => {
                    let mut connection_state = self.shared.state.lock().await;

                    let stream = match connection_state.streams.get_mut(&stream_id) {
                        Some(s) => s,
                        None => {
                            // Most likely the stream or connection was killed before we were able to send the
                            // response. Ok to ignore.
                            continue;
                        }
                    };

                    // NOTE: This should never fail as we only ever run the processing task once.
                    let (request_method, outgoing_body) = stream.outgoing_response_handler.take()
                        .ok_or_else(|| err_msg("Multiple responses received to a stream"))?;

                    let body = encode_response_body_v2(
                        request_method, &mut response.head, response.body);

                    // TODO: Deduplicate with the regular code.
                    let local_end = {
                        if let Some(body) = body {
                            // NOTE: Because we are still blocking the writing thread later down in this function,
                            // this won't trigger DATA frames to be sent until the HEADERS frame will be sent.
                            stream.processing_tasks.push(ChildTask::spawn(outgoing_body.run(body)));
                            false                            
                        } else {
                            // Mark as locally closed.
                            let mut stream_state = stream.state.lock().await;
                            stream_state.sending_end = true;
                            stream.sending_end_flushed = true;

                            true
                        }
                    };

                    let max_remote_frame_size = connection_state.remote_settings[SettingId::MAX_FRAME_SIZE] as usize;

                    drop(connection_state);

                    // TODO: Verify that whenever we start encoding headers, we definately send them
                    let header_block = encode_response_headers_block(
                        &response.head, &mut local_header_encoder)?;

                    write_headers_block(writer.as_mut(), stream_id, &header_block, local_end,
                                        max_remote_frame_size).await?;

                }
                ConnectionEvent::Closing { error } => {
                    if let Some(error) = error {
                        return Err(error.into());
                    } else {
                        // Fully flush any outgoing packets.
                        // TODO: Make the wait relative to the last write.
                        // TODO: Ignore connection closed errors?
                        writer.flush().await?;
                        common::wait_for(std::time::Duration::from_secs(1)).await;
                        return Ok(());
                    }
                }
                ConnectionEvent::Goaway { last_stream_id, error } => {
                    // TODO: Should we break out of the thread when we send this.
                    writer.write_all(&frame_utils::new_goaway_frame(last_stream_id, error.clone())).await?;
                    writer.flush().await?;
                    common::wait_for(std::time::Duration::from_secs(1)).await;

                    if error.code == ErrorCode::NO_ERROR {
                        let connection_state = self.shared.state.lock().await;
                        // TODO: Ensure that the read thread doesn't accept any more streams.
                        if connection_state.streams.is_empty() {
                            return Ok(());
                        }
                    } else {
                        return Err(error.into());
                    }
                }
                ConnectionEvent::AcknowledgeSettings { header_table_size } => {
                    if let Some(value) = header_table_size {
                        local_header_encoder.set_protocol_max_size(value as usize);
                    }

                    writer.write_all(&frame_utils::new_settings_ack_frame()).await?;

                    remote_settings_known = true;
                }
                ConnectionEvent::ResetStream { stream_id, error } => {
                    println!("SENDING RST STREAM : {:?}", error);
                    writer.write_all(&frame_utils::new_rst_stream_frame(stream_id, error)).await?;
                }
                ConnectionEvent::Ping { ping_frame } => {
                    writer.write_all(&frame_utils::new_ping_frame(ping_frame.opaque_data, true)).await?;
                }
                ConnectionEvent::StreamRead { stream_id, count } => {
                    // NOTE: The stream level flow control is already updated in the IncomingStreamBody.
                    self.shared.state.lock().await.local_connection_window += count as WindowSize;

                    // When we have read received data we'll send an update to the remote endpoint of our progress.
                    // TODO: Ideally batch these so that individual reads can't be used to determine internal control
                    // flow state. 
                    writer.write_all(&frame_utils::new_window_update_frame(0, count)).await?;
                    if stream_id != 0 {
                        writer.write_all(&frame_utils::new_window_update_frame(stream_id, count)).await?;
                    }
                }
                ConnectionEvent::StreamReaderClosed { stream_id, stream_state } => {
                    // TODO:
                    // - Reclaim any connection/stream level quota.
                    // - Mark as reader_closed for future packets. 
                }
                // Write event:
                // - Happens on either remote flow control updates or 
                ConnectionEvent::StreamWrite { .. } => {
                    let mut connection_state_guard = self.shared.state.lock().await;
                    let connection_state = &mut *connection_state_guard;
                
                    // TODO: Consider limiting this if we think it is too large.
                    let max_remote_frame_size = connection_state.remote_settings[SettingId::MAX_FRAME_SIZE] as usize;
    
                    let mut next_frame = None;
    
                    // TODO: Ensure that whenever we receive window updates, we retry sending something.
                    if connection_state.remote_connection_window <= 0 {
                        continue;
                    }

                    for (stream_id, stream) in &mut connection_state.streams {
                        if stream.sending_end_flushed {
                            continue;
                        }
    
                        let mut stream_state = stream.state.lock().await;
    
                        let min_window = std::cmp::min(
                            connection_state.remote_connection_window,
                            stream_state.remote_window);
                        if min_window < 0 {
                            continue;
                        }
    
                        let n_raw = std::cmp::min(min_window as usize, stream_state.sending_buffer.len());
                        let n = std::cmp::min(n_raw, max_remote_frame_size as usize);
                        
                        if n == 0 && !stream_state.sending_end {
                            continue;
                        }

                        stream_state.remote_window -= n as WindowSize;
                        connection_state.remote_connection_window -= n as WindowSize;
    
                        let remaining = stream_state.sending_buffer.split_off(n);
                        let frame_data = stream_state.sending_buffer.clone();
                        stream_state.sending_buffer = remaining;

                        let _ = stream.write_available_notifier.try_send(());

                        stream.sending_end_flushed = stream_state.sending_end && stream_state.sending_buffer.is_empty();

                        next_frame = Some((
                            *stream_id, frame_data, stream_state.sending_trailers.take(),
                            stream.sending_end_flushed, stream.is_closed(&stream_state)));

                        break;
                    }

                    // TODO: Should we ignore stream errors if the stream is already marked as closed based on
                    // received_at_end and sending_at_end
    
                    // Write out the next frame.
                    // TODO: To avoid so much copying, consider never sending until we have one full 'chunk' of data.
                    if let Some((stream_id, frame_data, trailers,
                                 end_stream, stream_closed)) = next_frame {
                        if stream_closed {
                            self.shared.finish_stream(connection_state, stream_id, None).await;
                        }

                        // Ensure all locks are dropped before we perform a blocking write.
                        drop(connection_state_guard);

                        if frame_data.len() > 0 || (trailers.is_none()) {
                            let frame = frame_utils::new_data_frame(
                                stream_id, frame_data, end_stream && trailers.is_none());
                            writer.write_all(&frame).await?;
                        }

                        if let Some(trailers) = trailers {
                            // Write the trailers (will always had end_of_strema)
                            let header_block = encode_trailers_block(&trailers, &mut local_header_encoder);
                            write_headers_block(
                                &mut *writer, stream_id, &header_block, end_stream, max_remote_frame_size).await?;
                        }
                    }
                }
                ConnectionEvent::StreamWriteFailure { stream_id, internal_error } => {
                    let mut connection_state = self.shared.state.lock().await;
                    self.shared.finish_stream(&mut connection_state, stream_id, Some(ProtocolErrorV2 {
                        code: ErrorCode::INTERNAL_ERROR,
                        message: "Internal error occured while sending data",
                        local: true
                    })).await;
                }
            }
        }
    }

    // TODO: Have a better way of cancelling this as we can't partially close this as it may set the error without
    // ending the Goaway?
    async fn wait_for_settings_ack(shared: Arc<ConnectionShared>) {
        common::wait_for(std::time::Duration::from_secs(SETTINGS_ACK_TIMEOUT_SECS)).await;

        // NOTE: This
        let error = ProtocolErrorV2 {
            code: ErrorCode::SETTINGS_TIMEOUT,
            message: "Settings took too long to be acknowledged",
            local: true
        };

        let mut connection_state = shared.state.lock().await;
        if connection_state.error.is_some() {
            // TODO: Also check that the error is not OK
            return;
        }

        let last_stream_id = connection_state.last_received_stream_id;

        connection_state.error = Some(error.clone());
        drop(connection_state);

        // TODO: Must coordinate this with the reader thread.
        let _ = shared.connection_event_sender.try_send(ConnectionEvent::Goaway {
            last_stream_id,
            error
        });
    }
    
}