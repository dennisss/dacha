use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

use common::errors::*;
use common::io::{IoError, IoErrorKind, Readable, Writeable};
use executor::child_task::ChildTask;
use executor::lock_async;

use crate::hpack;
use crate::proto::v2::*;
use crate::v2::body::{encode_request_body_v2, encode_response_body_v2};
use crate::v2::connection_shared::*;
use crate::v2::connection_state::*;
use crate::v2::frame_utils;
use crate::v2::headers::*;
use crate::v2::types::*;

/*
    Streams will be one in one of a few odd temporary states:
    - New stream created by  Client: pending
        => the client task can block on getting th result.
            We mainly need to later on send it back a
    - New stream received by Server: pending getting a response
        => We'll create a new task to generate the response.
        => later that initial task will end and instead become the sending task
*/

pub(super) struct ConnectionWriter {
    shared: Arc<ConnectionShared>,
}

impl ConnectionWriter {
    pub(super) fn new(shared: Arc<ConnectionShared>) -> Self {
        Self { shared }
    }

    // TODO: Catch BrokenPipe and ConnectionAborted errors.
    // TODO: If this returns a write error, we should prioritize other errors first.
    // e.g. If there is an enqueued Closing event with an error, then we should
    // prefer to return that error over others..

    pub async fn run(
        self,
        mut writer: Box<dyn Writeable>,
        upgrade_payload: Option<Box<dyn Readable>>,
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
            let mut state = self.shared.state.lock().await?.enter();

            connection_event_receiver = state
                .connection_event_receiver
                .take()
                .ok_or(err_msg("Multiple ConnectionWriters started?"))?;

            let mut payload = vec![];
            state
                .local_pending_settings
                .serialize_payload(&state.local_settings, &mut payload);

            state.local_settings_ack_waiter = Some(ChildTask::spawn(Self::wait_for_settings_ack(
                self.shared.clone(),
            )));
            remote_settings_known = state.remote_settings_known;
            state.exit();

            // Write out the initial settings frame.
            let mut frame = vec![];
            FrameHeader {
                length: payload.len() as u32,
                typ: FrameType::SETTINGS,
                flags: 0,
                reserved: 0,
                stream_id: 0,
            }
            .serialize(&mut frame)
            .unwrap();
            frame.extend(payload);
            writer.write_all(&frame).await?;
        }

        // Used to encode locally created headers to be sent to the other endpoint.
        // NOTE: This is shared across all streams on the connection.
        // TODO: COnsiderate this with the above lock.
        let mut local_header_encoder = {
            let state = self.shared.state.lock().await?.read_exclusive();
            hpack::Encoder::new(state.remote_settings[SettingId::HEADER_TABLE_SIZE] as usize)
        };
        local_header_encoder.set_local_max_size(self.shared.options.max_local_encoder_table_size);

        // List of events which we've deferred processing due to remote settings not
        // being known yet.
        let mut pending_events: Vec<ConnectionEvent> = vec![];

        // TODO: Once we fail, we should consider cleaning out all remaining events in
        // the channel and make sure that any pending requests to be sent are
        // properly rejected with the right error code/message (indicating that
        // are retryable and weren't really sent)
        /*
        TODO: The desired behavior is that if there are too many requests pending, we should queue
        the next request. We could have an opt-in solution to failing if queuing is required.
        - We should also be able to query the connection to check how many requests/streams are pending

        */

        loop {
            // TODO: If we are gracefully shutting down, stop waiting for events once all
            // pending streams have been closed.

            let event = {
                if remote_settings_known && !pending_events.is_empty() {
                    pending_events.remove(0)
                } else {
                    connection_event_receiver.recv().await?
                }
            };

            // Until we receive the first settings acknowledgement, we will queue all
            // events. TODO: Find a better solution than this.
            if !remote_settings_known {
                let allow = match event {
                    ConnectionEvent::AcknowledgeSettings { .. } => true,
                    ConnectionEvent::Closing { .. } => true,
                    _ => false,
                };

                if !allow {
                    pending_events.push(event);
                    continue;
                }
            }

            let now = SystemTime::now();

            match event {
                // TODO: Instead alwas enqueue requests and always
                ConnectionEvent::SendRequest => {
                    // TODO: If anything in here fails, we should report it to the requester rather
                    // than killing the whole thread.
                    // ^ We also need to let any listeners know that the request was dropped.

                    let mut connection_state = self.shared.state.lock().await?.enter();

                    // Checking that we are able to send a stream.
                    let remote_stream_limit = std::cmp::min(
                        connection_state.remote_settings[SettingId::MAX_CONCURRENT_STREAMS],
                        self.shared.options.max_outgoing_streams as u32,
                    );
                    if connection_state.local_stream_count >= remote_stream_limit as usize {
                        connection_state.exit();
                        continue;
                    }

                    // TODO: Take the request with the first non-cancelled response_handle
                    let (mut request, response_sender) = {
                        let mut ret = None;
                        while let Some(req) = connection_state.pending_requests.pop_front() {
                            // As an optimization, we'll skip requests that have their response
                            // handlers cancelled early.
                            if req.response_sender.is_closed() {
                                continue;
                            }

                            ret = Some((req.request, req.response_sender));
                            break;
                        }

                        if let Some(v) = ret {
                            v
                        } else {
                            connection_state.exit();
                            continue;
                        }
                    };

                    let body = encode_request_body_v2(&mut request.head, request.body);

                    // Generate a new stream id.
                    let stream_id = {
                        if connection_state.last_sent_stream_id == 0 {
                            if self.shared.is_server {
                                2
                            } else {
                                1
                            }
                        } else {
                            connection_state.last_sent_stream_id + 2
                        }
                    };
                    connection_state.last_sent_stream_id = stream_id;

                    let (mut stream, incoming_body, outgoing_body) =
                        self.shared.new_stream(&connection_state, stream_id);

                    let response_sender = {
                        let event_sender = self.shared.connection_event_sender.clone();
                        response_sender.with_cancellation_callback(move || async move {
                            let _ = event_sender
                                .send(ConnectionEvent::CancelRequest { stream_id })
                                .await;
                        })
                    };

                    // The type of request will determine if we allow a response

                    // Apply client request specific details to the stream's state.
                    let local_end = {
                        // TODO: If the response_handler is somehow dropped, then we
                        stream.incoming_response_handler =
                            Some((request.head.method, response_sender, incoming_body));

                        // TODO: Deduplicate this logic with the other side.
                        // TODO: Also ensure that the case of zero length bodies is optimized when
                        // it comes to intermediate body compression layers.
                        if let Some(body) = body {
                            // NOTE: Because we are still blocking the writing thread later down in
                            // this function, this won't trigger DATA
                            // frames to be sent until the HEADERS frame will be sent.
                            stream
                                .processing_tasks
                                .push(ChildTask::spawn(outgoing_body.run(body)));
                            false
                        } else {
                            // TODO: Lock and mark as locally closed?
                            let mut stream_state = stream.state.lock().await?.enter();
                            stream_state.sending_end = true;
                            stream_state.exit();

                            stream.sending_end_flushed = true;

                            true
                        }
                    };

                    connection_state.local_stream_count += 1;
                    connection_state.streams.insert(stream_id, stream);

                    let max_remote_frame_size =
                        connection_state.remote_settings[SettingId::MAX_FRAME_SIZE] as usize;

                    connection_state.last_user_byte_sent_time = now;

                    connection_state.exit();

                    // We are now done setting up the stream.
                    // Now we should just send the request to the other side.

                    // TODO: Write a unit test to ensure that if there is a single malformed
                    // request, the request still works.

                    // TODO: Split this up into the request validation and header encoding
                    // components so that we can return errors if it is invalid.
                    let header_block =
                        encode_request_headers_block(&request.head, &mut local_header_encoder)?;

                    /*
                    if header_block.len()
                        > (connection_state.remote_settings[SettingId::MAX_HEADER_LIST_SIZE]
                            as usize)
                    {
                        // Return an error to the request sender.

                        // We also need to revert changes to the
                        // local_header_encoder (instead it would probably be
                        // easier to approximate the size of the header block
                        // based on the raw size and add some overhead).
                    }
                    */

                    write_headers_block(
                        writer.as_mut(),
                        stream_id,
                        &header_block,
                        local_end,
                        max_remote_frame_size,
                    )
                    .await?;
                }
                ConnectionEvent::CancelRequest { stream_id } => {
                    let mut connection_state = self.shared.state.lock().await?.enter();
                    self.shared
                        .finish_stream(
                            &mut connection_state,
                            stream_id,
                            Some(ProtocolErrorV2 {
                                code: ErrorCode::CANCEL,
                                message: "Request cancelled.",
                                local: true,
                            }),
                        )
                        .await?;
                    connection_state.exit();
                }
                ConnectionEvent::SendPushPromise { request, response } => {}
                ConnectionEvent::SendResponse {
                    stream_id,
                    mut response,
                } => {
                    let mut connection_state = self.shared.state.lock().await?.enter();

                    let stream = match connection_state.streams.get_mut(&stream_id) {
                        Some(s) => s,
                        None => {
                            // Most likely the stream or connection was killed before we were able
                            // to send the response. Ok to ignore.
                            connection_state.exit();
                            continue;
                        }
                    };

                    // NOTE: This should never fail as we only ever run the processing task once.
                    let (request_method, outgoing_body) =
                        stream
                            .outgoing_response_handler
                            .take()
                            .ok_or_else(|| err_msg("Multiple responses received to a stream"))?;

                    let body =
                        encode_response_body_v2(request_method, &mut response.head, response.body);

                    // TODO: Deduplicate with the regular code.
                    let local_end = {
                        if let Some(body) = body {
                            // NOTE: Because we are still blocking the writing thread later down in
                            // this function, this won't trigger DATA
                            // frames to be sent until the HEADERS frame will be sent.
                            stream
                                .processing_tasks
                                .push(ChildTask::spawn(outgoing_body.run(body)));
                            false
                        } else {
                            // Mark as locally closed.
                            let mut stream_state = stream.state.lock().await?.enter();
                            stream_state.sending_end = true;
                            stream.sending_end_flushed = true;
                            stream_state.exit();

                            true
                        }
                    };

                    let max_remote_frame_size =
                        connection_state.remote_settings[SettingId::MAX_FRAME_SIZE] as usize;

                    connection_state.last_user_byte_sent_time = now;

                    connection_state.exit();

                    // TODO: Verify that whenever we start encoding headers, we definately send them
                    let header_block =
                        encode_response_headers_block(&response.head, &mut local_header_encoder)?;

                    write_headers_block(
                        writer.as_mut(),
                        stream_id,
                        &header_block,
                        local_end,
                        max_remote_frame_size,
                    )
                    .await?;
                }
                // If the reading thread closes,
                // The reader thread could either have a legitamite error or it could have had an
                // uncaught failure:
                // - Uncaught failures must be propagated, other messages shouldn't be
                ConnectionEvent::Closing {
                    send_goaway,
                    close_with,
                } => {
                    let mut should_close = close_with.is_some();

                    let last_stream_id = {
                        let connection_state = self.shared.state.lock().await?.enter();

                        // NOTE: It is illegal to send a Closing event unless you mark the
                        // shutting_down state as non-No.
                        assert!(connection_state.shutting_down.is_some());

                        let id = connection_state.upper_received_stream_id;

                        connection_state.exit();

                        id
                    };

                    if let Some(error) = send_goaway {
                        // TODO: If this errors out, should we prefer to return the close_with error
                        // if available?
                        should_close |= error.code != ErrorCode::NO_ERROR;
                        writer
                            .write_all(&frame_utils::new_goaway_frame(last_stream_id, error))
                            .await?;

                        // TODO: Figure out if there is an upper bound to how long this flush will
                        // time. If it is not fast, then it will block
                        // shutdown for a while.
                        writer.flush().await?;
                    }

                    if self.shared.is_server {
                        // On the server, we can close once there are no outstanding streams and it
                        // is impossible for the client to send more
                        // requests. Currently this logic doesn't make a
                        // different as we always set close_with when decreasing the
                        // upper_received_stream_id

                        let connection_state = self.shared.state.lock().await?.enter();

                        if connection_state.streams.is_empty()
                            && connection_state.upper_received_stream_id
                                == connection_state.last_received_stream_id
                        {
                            should_close = true;
                        }

                        connection_state.exit();
                    } else {
                        // In the client, we will close the connection as soon as all streams have
                        // finished running.

                        let connection_state = self.shared.state.lock().await?.enter();

                        if connection_state.streams.is_empty() {
                            should_close = true;
                        }

                        connection_state.exit();
                    }

                    if should_close {
                        // TODO: Validate that close_with is present if we didn't get a NO_ERROR.

                        // Give the OS some time to fully flush the outgoing GOAWAY packet.
                        // TODO: Make the wait relative to the last write.
                        executor::sleep(std::time::Duration::from_millis(500)).await;
                        return close_with.unwrap_or(Ok(()));
                    }
                }
                ConnectionEvent::AcknowledgeSettings { header_table_size } => {
                    if let Some(value) = header_table_size {
                        local_header_encoder.set_protocol_max_size(value as usize);
                    }

                    writer
                        .write_all(&frame_utils::new_settings_ack_frame())
                        .await?;

                    remote_settings_known = true;
                }
                ConnectionEvent::ResetStream { stream_id, error } => {
                    println!(
                        "[http::v2] Sending RST_STREAM : (sid {}) {}",
                        stream_id, error
                    );
                    writer
                        .write_all(&frame_utils::new_rst_stream_frame(stream_id, error))
                        .await?;
                }
                ConnectionEvent::Ping { ping_frame } => {
                    writer
                        .write_all(&frame_utils::new_ping_frame(ping_frame.opaque_data, true))
                        .await?;
                }
                ConnectionEvent::StreamRead { stream_id, count } => {
                    // NOTE: The stream level flow control is already updated in the
                    // IncomingStreamBody.
                    self.shared
                        .state
                        .apply(|s| s.local_connection_window += count as WindowSize)
                        .await?;

                    // TODO: Only send these if there is a meaningfully large change in the window
                    // size to report.

                    // When we have read received data we'll send an update to the remote endpoint
                    // of our progress. TODO: Ideally batch these so that
                    // individual reads can't be used to determine internal control
                    // flow state.
                    let mut out = vec![];
                    frame_utils::new_window_update_frame(0, count, &mut out);

                    if stream_id != 0 {
                        frame_utils::new_window_update_frame(stream_id, count, &mut out);
                    }

                    writer.write_all(&out).await?;
                }
                ConnectionEvent::StreamReaderCancelled { stream_id } => {
                    let mut connection_state = self.shared.state.lock().await?.enter();
                    self.shared
                        .handle_stream_reader_closed(&mut connection_state, stream_id)
                        .await?;
                    connection_state.exit();
                }
                // Write event:
                // - Happens on either remote flow control updates or
                ConnectionEvent::StreamWrite { .. } => {
                    let mut connection_state_guard = self.shared.state.lock().await?.enter();
                    let connection_state = &mut *connection_state_guard;

                    // TODO: Consider limiting this if we think it is too large.
                    let max_remote_frame_size =
                        connection_state.remote_settings[SettingId::MAX_FRAME_SIZE] as usize;

                    let mut next_frame = None;

                    // TODO: Ensure that whenever we receive window updates, we retry sending
                    // something.
                    if connection_state.remote_connection_window <= 0 {
                        connection_state_guard.exit();
                        continue;
                    }

                    for (stream_id, stream) in &mut connection_state.streams {
                        if stream.sending_end_flushed {
                            continue;
                        }

                        let mut stream_state = stream.state.lock().await?.enter();

                        let min_window = std::cmp::min(
                            connection_state.remote_connection_window,
                            stream_state.remote_window,
                        );

                        if min_window < 0 {
                            stream_state.exit();
                            continue;
                        }

                        let n_raw =
                            std::cmp::min(min_window as usize, stream_state.sending_buffer.len());
                        let n = std::cmp::min(n_raw, max_remote_frame_size as usize);

                        stream_state.remote_window -= n as WindowSize;
                        connection_state.remote_connection_window -= n as WindowSize;

                        // Split off the first n bytes of the sending_buffer as the next DATA
                        // frame to send.
                        let frame_data = {
                            let remaining = stream_state.sending_buffer.split_off(n);
                            let frame_data = stream_state.sending_buffer.clone();
                            stream_state.sending_buffer = remaining;
                            frame_data
                        };

                        if n > 0 {
                            let _ = stream.write_available_notifier.try_send(());
                        }

                        let body_done =
                            stream_state.sending_end && stream_state.sending_buffer.is_empty();

                        // Trailers we will send in the current iteration.
                        // Note that we can only send trailers once all the buffered body data is
                        // sent.
                        let trailers_to_send = {
                            if body_done {
                                stream_state.sending_trailers.take()
                            } else {
                                None
                            }
                        };

                        stream.sending_end_flushed = body_done;

                        // Skip if there is no information to send to the other side.
                        if n == 0 && !stream.sending_end_flushed {
                            stream_state.exit();
                            continue;
                        }

                        next_frame = Some((
                            *stream_id,
                            frame_data,
                            trailers_to_send,
                            stream.sending_end_flushed,
                            stream.is_closed(&stream_state),
                        ));

                        stream_state.exit();
                        break;
                    }

                    if next_frame.is_some() {
                        connection_state.last_user_byte_sent_time = now;
                    }

                    // TODO: Should we ignore stream errors if the stream is already marked as
                    // closed based on received_at_end and sending_at_end

                    // Write out the next frame.
                    // TODO: To avoid so much copying, consider never sending until we have one full
                    // 'chunk' of data.
                    if let Some((stream_id, frame_data, trailers, end_stream, stream_closed)) =
                        next_frame
                    {
                        if stream_closed {
                            self.shared
                                .finish_stream(connection_state, stream_id, None)
                                .await?;
                        }

                        // Ensure all locks are dropped before we perform a blocking write.
                        connection_state_guard.exit();

                        if frame_data.len() > 0 || (trailers.is_none()) {
                            let frame = frame_utils::new_data_frame(
                                stream_id,
                                frame_data,
                                end_stream && trailers.is_none(),
                            );
                            writer.write_all(&frame).await?;
                        }

                        if let Some(trailers) = trailers {
                            // Write the trailers (will always had end_of_stream)
                            assert!(end_stream);
                            let header_block =
                                encode_trailers_block(&trailers, &mut local_header_encoder);
                            write_headers_block(
                                &mut *writer,
                                stream_id,
                                &header_block,
                                end_stream,
                                max_remote_frame_size,
                            )
                            .await?;
                        }

                        // Schedule to try sending more data as we most likely how more that we can
                        // send.
                        let _ = self
                            .shared
                            .connection_event_sender
                            .try_send(ConnectionEvent::StreamWrite { stream_id: 0 });

                        // TODO: Immediately retry as we may have more data to
                        // send? The main caveat is that
                        // we should prioritze
                    } else {
                        connection_state_guard.exit();
                    }
                }
                ConnectionEvent::StreamWriteFailure {
                    stream_id,
                    internal_error,
                } => {
                    if let Some(IoError {
                        kind: IoErrorKind::Cancelled,
                        ..
                    }) = internal_error.downcast_ref()
                    {
                        lock_async!(connection_state <= self.shared.state.lock().await?, {
                            self.shared
                                .handle_stream_writer_closed(&mut connection_state, stream_id)
                                .await
                        })?;

                        continue;
                    }

                    // TODO: Find a better way to export this error.
                    eprintln!("HTTP2 Stream Write Failure: {}", internal_error);

                    let proto_error = ProtocolErrorV2 {
                        code: ErrorCode::INTERNAL_ERROR,
                        message: "Internal error occured while sending data",
                        local: true,
                    };

                    let mut connection_state = self.shared.state.lock().await?.enter();
                    self.shared
                        .finish_stream(&mut connection_state, stream_id, Some(proto_error))
                        .await?;
                    connection_state.exit();
                }
            }
        }
    }

    /// NOTE: The task running this can only be cancelled if a ConnectionState
    /// lock is held. This enables this to operate atomically as we also
    /// perform all operations after the timeout under a ConnectionState
    /// lock.
    async fn wait_for_settings_ack(shared: Arc<ConnectionShared>) {
        executor::sleep(shared.options.settings_ack_timeout.clone()).await;

        let error = ProtocolErrorV2 {
            code: ErrorCode::SETTINGS_TIMEOUT,
            message: "Settings took too long to be acknowledged",
            local: true,
        };

        let mut connection_state = shared.state.lock().await.unwrap().enter();
        if let ShuttingDownState::Complete = connection_state.shutting_down {
            connection_state.exit();
            return;
        }

        // Mark that no more streams should be received by the reader.
        connection_state.upper_received_stream_id = connection_state.last_received_stream_id;

        connection_state
            .set_shutting_down(ShuttingDownState::Complete)
            .await;

        let _ = shared
            .connection_event_sender
            .try_send(ConnectionEvent::Closing {
                send_goaway: Some(error.clone()),
                close_with: Some(Err(error.into())),
            });

        connection_state.exit();
    }
}
