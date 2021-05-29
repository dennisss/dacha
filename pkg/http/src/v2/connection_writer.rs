use std::{convert::TryFrom, sync::Arc};
use std::collections::HashMap;

use common::{chrono::Duration, errors::*};
use common::io::{Writeable, Readable};
use common::async_std::channel;
use common::async_std::sync::Mutex;
use common::async_std::task;
use common::chrono::prelude::*;
use common::task::ChildTask;

use crate::v2::types::*;
use crate::v2::body::*;
use crate::v2::stream::*;
use crate::v2::stream_state::*;
use crate::v2::headers::*;
use crate::v2::connection_state::*;
use crate::{headers::connection, method::Method, v2::settings::*};
use crate::hpack::HeaderFieldRef;
use crate::hpack;
use crate::request::Request;
use crate::response::{Response, ResponseHead};
use crate::server::RequestHandler;
use crate::proto::v2::*;
use crate::v2::frame_utils;
use crate::v2::connection_shared::*;

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

        {
            let mut state = self.shared.state.lock().await;

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
                    self.shared.connection_event_channel.1.recv().await?
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

            println!("Got ConnectionEvent!");

            match event {
                // TODO: Instead alwas enqueue requests and always 
                ConnectionEvent::SendRequest => {
                    // TODO: Only allow if we are a client.
                    
                    println!("Sending request...");

                    // TODO: If anything in here fails, we should report it to the requester rather than
                    // killing the whole thread.

                    let mut connection_state = self.shared.state.lock().await;

                    // Checking that we are able to send a stream.
                    let remote_stream_limit = connection_state.remote_settings[SettingId::MAX_CONCURRENT_STREAMS];
                    if connection_state.local_stream_count >= remote_stream_limit as usize {
                        continue;
                    }

                    let (request, response_handler) = match connection_state.pending_requests.pop_front() {
                        Some(v) => (v.request, v.response_handler),
                        None => continue
                    };

                    // TODO: Write the rest of the headers (all names should be converted to ascii lowercase)
                    // (aside get a reference from the RFC)

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

                        // TODO: Verify that compression layers have a known length for a known length underlying stream
                        // (or just don't encode zero length streams or really anything very tiny)
                        if request.body.len() == Some(0) {
                            // TODO: Lock and mark as locally closed?
                            let mut stream_state = stream.state.lock().await;
                            stream_state.sending_at_end = true;

                            true
                        } else {
                            // NOTE: Because we are still blocking the writing thread later down in this function,
                            // this won't trigger DATA frames to be sent until the HEADERS frame will be sent.
                            stream.processing_tasks.push(ChildTask::spawn(outgoing_body.run(request.body)));
                            false
                        }
                    };

                    connection_state.local_stream_count += 1;
                    connection_state.streams.insert(stream_id, stream);

                    // 
                
                    /*
                        Streams will be one in one of a few odd temporary states:
                        - New stream created by  Client: pending 
                            => the client task can block on getting th result.
                               We mainly need to later on send it back a 
                        - New stream received by Server: pending getting a response
                            => We'll create a new task to generate the response.
                            => later that initial task will end and instead become the sending task
                    */

                    let max_remote_frame_size = connection_state.remote_settings[SettingId::MAX_FRAME_SIZE] as usize;

                    drop(connection_state);

                    // We are now done setting up the stream.
                    // Now we should just send the request to the other side.

                    let header_block = encode_request_headers_block(
                        &request.head, &mut local_header_encoder)?;
                    write_headers_block(writer.as_mut(), stream_id, &header_block, local_end,
                                        max_remote_frame_size).await?;

                    println!("Done request send!");
                }
                ConnectionEvent::SendPushPromise { request, response } => {

                }
                ConnectionEvent::SendResponse { stream_id, response } => {
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
                    let outgoing_body = stream.outgoing_response_handler.take()
                        .ok_or_else(|| err_msg("Multiple responses received to a stream"))?;

                    // TODO: Deduplicate with the regular code.
                    let local_end = {
                        // TODO: Verify that compression layers have a known length for a known length underlying stream
                        // (or just don't encode zero length streams or really anything very tiny)
                        if response.body.len() == Some(0) {
                            // TODO: Lock and mark as locally closed?
                            let mut stream_state = stream.state.lock().await;
                            stream_state.sending_at_end = true;

                            true
                        } else {
                            // NOTE: Because we are still blocking the writing thread later down in this function,
                            // this won't trigger DATA frames to be sent until the HEADERS frame will be sent.
                            stream.processing_tasks.push(ChildTask::spawn(outgoing_body.run(response.body)));
                            false
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
                        common::wait_for(std::time::Duration::from_secs(1));
                        return Ok(());
                    }
                }
                ConnectionEvent::Goaway { last_stream_id, error } => {
                    // TODO: Should we break out of the thread when we send this.
                    writer.write_all(&frame_utils::new_goaway_frame(last_stream_id, error.clone())).await?;
                    writer.flush().await?;
                    common::wait_for(std::time::Duration::from_secs(1));

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
                // Write event:
                // - Happens on either remote flow control updates or 
                ConnectionEvent::StreamWrite { .. } => {
                    let mut connection_state_guard = self.shared.state.lock().await;
                    let connection_state = &mut *connection_state_guard;
                
                    let max_remote_frame_size = connection_state.remote_settings[SettingId::MAX_FRAME_SIZE] as usize;
    
                    let mut next_frame = None;
    

                    for (stream_id, stream) in &mut connection_state.streams {
                        if connection_state.remote_connection_window <= 0 {
                            break;
                        }

                        if stream.sent_end_of_stream {
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
                        
                        if n == 0 && !stream_state.sending_at_end {
                            continue;
                        }

                        stream_state.remote_window -= n as WindowSize;
                        connection_state.remote_connection_window -= n as WindowSize;
    
                        let remaining = stream_state.sending_buffer.split_off(n);
                        let frame_data = stream_state.sending_buffer.clone();
                        stream_state.sending_buffer = remaining;

                        let _ = stream.write_available_notifier.try_send(());

                        stream.sent_end_of_stream = stream_state.sending_at_end;

                        next_frame = Some((*stream_id, frame_data, stream_state.sending_trailers.take(), stream_state.sending_at_end, stream_state.is_closed()));

                        break;
                    }

                    // TODO: Should we ignore stream errors if the stream is already marked as closed based on
                    // received_at_end and sending_at_end
    
                    // Write out the next frame.
                    // TODO: To avoid so much copying, consider never sending until we have one full 'chunk' of data.
                    if let Some((stream_id, frame_data, trailers, end_stream, stream_closed)) = next_frame {
                        if stream_closed {
                            self.shared.finish_stream(connection_state, stream_id);
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
            }
        }
    }

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
        let _ = shared.connection_event_channel.0.try_send(ConnectionEvent::Goaway {
            last_stream_id,
            error
        });
    }
    
}