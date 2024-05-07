use std::{convert::TryFrom, sync::Arc};

use common::errors::Result;
use executor::sync::{AsyncMutex, PoisonError};
use executor::{channel, lock_async};

use crate::proto::v2::*;
use crate::request::Request;
use crate::server_handler::{ServerHandler, ServerRequestContext};
use crate::v2::body::*;
use crate::v2::connection_state::*;
use crate::v2::options::{ConnectionOptions, ServerConnectionOptions};
use crate::v2::stream::*;
use crate::v2::stream_state::*;
use crate::v2::types::*;

pub const CONNECTION_PREFACE: &[u8] = b"PRI * HTTP/2.0\r\n\r\nSM\r\n\r\n";

pub const CONNECTION_PREFACE_BODY: &[u8] = b"SM\r\n\r\n";

pub(super) struct ConnectionShared {
    pub is_server: bool,

    pub state: AsyncMutex<ConnectionState>,

    // TODO: We may want to keep around a timer for the last time we closed a stream so that if we
    /// Handler for producing responses to incoming requests.
    ///
    /// NOTE: This will only be used in HTTP servers.
    pub server_options: Option<ServerConnectionOptions>,

    /// Used to notify the connection of events that have occured.
    /// The writer thread listens to these events performs actions such as
    /// sending more data, starting requests, etc. in response to each
    /// event.
    ///
    /// TODO: Make this a bounded channel?
    pub connection_event_sender: channel::Sender<ConnectionEvent>,

    /// TODO: Eventually support changing this.
    pub options: ConnectionOptions,
}

impl ConnectionShared {
    pub fn is_local_stream_id(&self, id: StreamId) -> bool {
        // Clients have ODD numbered ids. Servers have EVEN numbered ids.
        self.is_server == (id % 2 == 0)
    }

    pub fn is_remote_stream_id(&self, id: StreamId) -> bool {
        !self.is_local_stream_id(id)
    }

    /// Called when the local reader for a stream has been dropped.
    pub async fn handle_stream_reader_closed(
        &self,
        connection_state: &mut ConnectionState,
        stream_id: StreamId,
    ) -> Result<(), PoisonError> {
        let mut stream = match connection_state.streams.get_mut(&stream_id) {
            Some(s) => s,
            _ => return Ok(()),
        };

        let is_closed = lock_async!(stream_state <= stream.state.lock().await?, {
            stream_state.reader_cancelled = true;

            if stream.is_closed(&stream_state) {
                return true;
            }

            // We aren't able to close the stream yet, but we should clear any remaining
            // received data and allow the remote side to send more.

            // TODO: Dedup this with finish_stream.
            if stream_state.received_buffer.len() > 0 {
                self.connection_event_sender
                    .send(ConnectionEvent::StreamRead {
                        stream_id,
                        count: stream_state.received_buffer.len(),
                    })
                    .await;
            }

            stream_state.received_buffer.clear();

            false
        });

        if is_closed {
            self.finish_stream(connection_state, stream_id, None)
                .await?;
        }

        Ok(())
    }

    pub async fn handle_stream_writer_closed(
        &self,
        connection_state: &mut ConnectionState,
        stream_id: StreamId,
    ) -> Result<(), PoisonError> {
        let mut stream = match connection_state.streams.get_mut(&stream_id) {
            Some(s) => s,
            _ => return Ok(()),
        };

        let is_closed = lock_async!(stream_state <= stream.state.lock().await?, {
            stream_state.writer_cancelled = true;

            // Clean up any unsent data.
            // NOTE: We don't update window sizes until data is removed from this buffer so
            // we don't need to worry about updating that.
            stream_state.sending_buffer.clear();
            stream_state.sending_trailers.take();

            stream.is_closed(&stream_state)
        });

        if is_closed {
            self.finish_stream(connection_state, stream_id, None)
                .await?;
        }

        Ok(())
    }

    /// Performs cleanup on a stream which is done being used.
    ///
    /// This should be called whenever stream.is_closed() transitions to true.
    ///
    /// NOTE: This internally locks the stream state so the caller must free any
    /// references to it.
    pub async fn finish_stream(
        &self,
        connection_state: &mut ConnectionState,
        stream_id: StreamId,
        additional_error: Option<ProtocolErrorV2>,
    ) -> Result<(), PoisonError> {
        // TODO: Verify that there are no cyclic references to Arc<StreamState>
        // (otherwise the stream state may never get freed)
        let mut stream = match connection_state.streams.remove(&stream_id) {
            Some(s) => s,
            // TODO: Should we complain in this instance?
            None => {
                // TODO: Don't send this on an idle stream (as that is not allowed)
                // ^ This may happen if we are a client that just got a request and we failed
                // to validate that the request is ok to pass to the other endpoint.
                if let Some(error) = additional_error {
                    if error.local {
                        let _ = self
                            .connection_event_sender
                            .send(ConnectionEvent::ResetStream { stream_id, error })
                            .await;
                    }
                }

                return Ok(());
            }
        };

        let mut stream_state = stream.state.lock().await?.enter();

        if let Some(error) = additional_error {
            if stream_state.error.is_none() {
                stream_state.error = Some(error);
                // TODO: Add a stream.read_available_notifier here.
            }
        }

        // TODO: Generalize this. If the stream isn't fully closed, we need to send and
        // error. NOTE: If this is true, then reader_closed should also be true.
        if stream_state.error.is_none() && !stream.is_normally_closed(&stream_state) {
            stream_state.error = Some(ProtocolErrorV2 {
                code: ErrorCode::CANCEL,
                message: "Stream no longer needed.",
                local: true,
            });
        }

        // Ensure that all events are propagated to the reader/writer threads.
        // TODO: Remove these and instead find all places where we neglected to add
        // these.
        let _ = stream.read_available_notifier.try_send(());
        let _ = stream.write_available_notifier.try_send(());

        if let Some(error) = stream_state.error.clone() {
            if error.local {
                // Notify the other endpoint of locally generated stream errors.
                let _ = self
                    .connection_event_sender
                    .send(ConnectionEvent::ResetStream {
                        stream_id,
                        error: error.clone(),
                    })
                    .await;
            }

            // If there is unread data when resetting a stream we can clear it and count it
            // as 'read' by increasing the connection level flow control limit.
            if stream_state.received_buffer.len() > 0 {
                self.connection_event_sender
                    .send(ConnectionEvent::StreamRead {
                        stream_id: 0,
                        // TODO: This will be an underestimate if we rejected any DATA frames (and
                        // thus never made it into this buffer)
                        count: stream_state.received_buffer.len(),
                    })
                    .await;
            }

            // Clear no longer needed memory.
            // TODO: Make sure that this doesn't happen if we are just gracefully closing a
            // stream.
            stream_state.received_buffer.clear();
            stream_state.sending_buffer.clear();

            while let Some(handle) = stream.processing_tasks.pop() {
                drop(handle);
            }

            // If the error happened before response headers will received by a client,
            // response with an error. TODO: Also need to notify the requester
            // of whether or not the request is trivially retryable
            // (based on the stream id in the latest GOAWAY message).
            //
            // TODO: Ensure that this is never present when the stream has a value.
            if let Some((request_method, response_sender, body)) =
                stream.incoming_response_handler.take()
            {
                response_sender.send(Err(error.into())).await;
            }

            if let Some(outgoing_body) = stream.outgoing_response_handler.take() {
                // I don't think I need to do anything here?
            }
        }

        // If we are in a graceful shutdown mode, trigger a full shutdown once we have
        // completed all streams.
        //
        // TODO: What should we do about promised streams?
        //
        // NOTE: If connection_state.shutting_down is not No, then it's likely an OK
        // error, because we close the connection immediately on other types of
        // errors.
        if connection_state.shutting_down.is_some() && connection_state.streams.is_empty() {
            // NOTE: Because both of the values are None, we leave the final decision on
            // whether or not to close to the writer thread.
            let _ = self
                .connection_event_sender
                .try_send(ConnectionEvent::Closing {
                    send_goaway: None,
                    close_with: None,
                });
        }

        if self.is_local_stream_id(stream_id) {
            connection_state.local_stream_count -= 1;

            if !self.is_server {
                if let Some(listener) = &connection_state.event_listener {
                    listener.handle_request_completed().await;
                }
            }
        } else {
            connection_state.remote_stream_count -= 1;
        }

        stream_state.exit();
        Ok(())
    }

    /// Constructs a new stream object along with coupled readers/writers for
    /// the stream's data.
    ///
    /// NOTE: This does NOT insert the stream into the ConnectionState. It's
    /// purely an object construction helper.
    pub fn new_stream(
        &self,
        connection_state: &ConnectionState,
        stream_id: StreamId,
    ) -> (Stream, IncomingStreamBody, OutgoingStreamBodyPoller) {
        // NOTE: These channels only act as a boolean flag of whether or not something
        // has changed so we should only need to ever have at most 1 message in
        // each of them.
        let (read_available_notifier, read_available_receiver) = channel::bounded(1);
        let (write_available_notifier, write_available_receiver) = channel::bounded(1);

        let stream = Stream {
            read_available_notifier,
            write_available_notifier,
            incoming_response_handler: None,
            outgoing_response_handler: None,
            processing_tasks: vec![],
            sending_end_flushed: false,
            state: Arc::new(AsyncMutex::new(StreamState {
                // weight: 16, // Default weight
                // dependency: 0,
                error: None,

                local_window: connection_state.local_settings[SettingId::INITIAL_WINDOW_SIZE]
                    as WindowSize,
                remote_window: connection_state.remote_settings[SettingId::INITIAL_WINDOW_SIZE]
                    as WindowSize,

                reader_cancelled: false,
                received_buffer: vec![],
                received_trailers: None,
                received_end: false,
                received_expected_bytes: None,
                received_total_bytes: 0,

                writer_cancelled: false,
                sending_buffer: vec![],
                sending_trailers: None,
                sending_end: false,
                max_sending_buffer_size: self.options.max_sending_buffer_size,
            })),
        };

        let incoming_body = IncomingStreamBody::new(
            stream_id,
            stream.state.clone(),
            self.connection_event_sender.clone(),
            read_available_receiver,
        );

        let outgoing_body = OutgoingStreamBodyPoller::new(
            stream_id,
            stream.state.clone(),
            self.connection_event_sender.clone(),
            write_available_receiver,
        );

        (stream, incoming_body, outgoing_body)
    }

    /// Wrapper around the server request handler which is intended to call the
    /// server request handler in a separate task and then notify the
    /// ConnectionWriter once a response is ready to be sent back to the
    /// client.
    pub async fn request_handler_driver(
        self: Arc<ConnectionShared>,
        stream_id: StreamId,
        request: Request,
    ) {
        let server_options = self.server_options.as_ref().unwrap();

        let context = ServerRequestContext {
            connection_context: &server_options.connection_context,
            // TODO: Add the stream id
        };

        let response = server_options
            .request_handler
            .handle_request(request, context)
            .await;

        let _ = self
            .connection_event_sender
            .send(ConnectionEvent::SendResponse {
                stream_id,
                response,
            })
            .await;

        // TODO: Consider starting the processing task for reading the outgoing
        // body here. This will require us to validate the stream is
        // still open, but this may help with latency.
    }
}
