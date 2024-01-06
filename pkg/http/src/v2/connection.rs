use std::collections::HashMap;
use std::future::Future;
use std::time::SystemTime;
use std::{convert::TryFrom, sync::Arc};

use common::chrono::prelude::*;
use common::hash::FastHasherBuilder;
use common::io::{IoError, IoErrorKind, Readable, Writeable};
use common::{chrono::Duration, errors::*};
use executor::channel;
use executor::child_task::ChildTask;
use executor::sync::Mutex;

use crate::connection_event_listener::ConnectionEventListener;
use crate::proto::v2::*;
use crate::request::Request;
use crate::response::{Response, ResponseHead};
use crate::response_channel::{new_response_channel, ResponseReceiver};
use crate::server_handler::ServerHandler;
use crate::v2::connection_reader::ConnectionReader;
use crate::v2::connection_shared::ConnectionShared;
use crate::v2::connection_state::*;
use crate::v2::connection_writer::ConnectionWriter;
use crate::v2::options::{ConnectionOptions, ServerConnectionOptions};
use crate::v2::types::*;
use crate::{headers::connection, method::Method, v2::settings::*};

// TODO: Use this.
const MAX_STREAM_ID: StreamId = (1 << 31) - 1;

/// Stream id used when
/// NOTE: This is a client stream id.
const UPGRADE_STREAM_ID: StreamId = 1;

/// Initial size of the connection flow control window at both endpoints.
///
/// NOTE: The connection frame control window is only updated on WINDOW_UPDATE
/// frames (not SETTINGS)
const INITIAL_CONNECTION_WINDOW_SIZE: WindowSize = 65535;

// TODO: Should also use PING to countinuously verify that the server is still
// alive.
//
//  Received a GOAWAY with error code ENHANCE_YOUR_CALM and debug data equal to
// "too_many_pings" https://github.com/grpc/grpc/blob/fd3bd70939fb4239639fbd26143ec416366e4157/doc/keepalive.md
//
//

/*
    Eventually we want to have a HTTP2 specific wrapper around a Request/Response to support
    changing settings, assessing stream/connection ids, or using the push functionality.
*/

/// Describes any past processing which has already happened on the connection
/// before it was handed to the HTTP2 'Connection' for further processing.
pub struct ConnectionInitialState {
    /// This is an HTTP server and we've already read the first line of the HTTP
    /// 2.0 preface from the client. The second half of the preface still
    /// needs to be read.
    ///
    /// This is a convenience feature that is for enabling the easy
    /// implementation of HTTP 2 on top of an existing HTTP 1 server which
    /// scans the request head and then upgrades if seeing an HTTP 2
    /// version.
    pub seen_preface_head: bool,

    /// We are upgrading using an HTTP 1.1 request/response.
    /// Usually this requires that some remaining data is written out to the
    /// stream before it can be used for HTTP 2. (e.g. the HTTP 1.1 request
    /// body or the HTTP 1.1 101 upgrade response). To support these
    /// requirements, this data can be passed in this state and
    /// the HTTP2 connection will ensure that this data is written prior to
    /// HTTP2 data.
    pub upgrade_payload: Option<Box<dyn Readable>>,
}

impl ConnectionInitialState {
    pub fn raw() -> Self {
        Self {
            seen_preface_head: false,
            upgrade_payload: None,
        }
    }
}

/*
Client API requests:
- When a connection dies, the client should know in order to be able to immediately clean it up and replace it

- But the client should not have a handle to the Connection

- The client should be smart enough to identify a lame duck server and discard the connection.


TODO: If the client has hit the max number of outstanding requests, that should also count as being in a Failing state.
    - Also implement this for the nested connections that implement accepting_requests().
*/

/*
Important points:
- Never increase the last-stream-id sent.
- Initially seng a NO_ERROR GOAWAY with 2^31 - 1 to initiate shutdown.
- Clients should respect this by not sending any more messages.

*/

// TODO: Make sure we signal a small enough value to the hpack encoder to be
// reasonable.

// TODO: Make sure we reject new streams when in a goaway state.

// TODO: Should we support allowing the connection itself to stay half open.

// TODO: Have a simpler request API with a direct reader/writer interface.

/// A single HTTP2 connection to a remote endpoint.
///
/// After
///
/// This object should be held for as long as the user wants to issue new
/// requests.
/// - On drop we will
///
/// On drop, we should trigger a preliminary GOAWAY so that the background
/// thread
pub struct Connection {
    shared: Arc<ConnectionShared>,
}

impl Drop for Connection {
    fn drop(&mut self) {
        // TODO: Make this send a GOAWAY with the max stream id so that we can still
        // accept new push requests but we stop the connection once done.

        let shared = self.shared.clone();

        // TODO: Optimize this away if the connection as already stopped.
        executor::spawn(async move {
            let _ = Self::shutdown_impl(&shared, true).await;
        });
    }
}

impl Connection {
    pub fn new(
        options: ConnectionOptions,
        server_options: Option<ServerConnectionOptions>,
    ) -> Self {
        let is_server = server_options.is_some();

        // TODO: Implement SETTINGS_MAX_HEADER_LIST_SIZE.
        // XXX: Yes

        let local_pending_settings = options.protocol_settings.clone();

        let (connection_event_sender, connection_event_receiver) = channel::unbounded();

        Connection {
            shared: Arc::new(ConnectionShared {
                is_server,
                options,
                server_options,
                connection_event_sender,
                state: Mutex::new(ConnectionState {
                    running: false,
                    shutting_down: ShuttingDownState::No,
                    connection_event_receiver: Some(connection_event_receiver),
                    local_settings: SettingsContainer::default(),
                    local_settings_ack_waiter: None,
                    local_pending_settings,
                    local_connection_window: INITIAL_CONNECTION_WINDOW_SIZE,
                    remote_settings: SettingsContainer::default(),
                    remote_settings_known: false,
                    remote_connection_window: INITIAL_CONNECTION_WINDOW_SIZE,
                    last_received_stream_id: 0,
                    last_sent_stream_id: 0,
                    upper_received_stream_id: MAX_STREAM_ID,
                    upper_sent_stream_id: MAX_STREAM_ID,
                    pending_requests: std::collections::VecDeque::new(),
                    local_stream_count: 0,
                    remote_stream_count: 0,

                    streams: HashMap::with_hasher(FastHasherBuilder::default()),
                    event_listener: None,

                    last_user_byte_received_time: SystemTime::now(),
                    last_user_byte_sent_time: SystemTime::now(),
                }),
            }),
        }
    }

    pub async fn set_event_listener(&self, event_listener: Box<dyn ConnectionEventListener>) {
        self.shared.state.lock().await.event_listener = Some(event_listener);
    }

    /// Called on a client which just sent a request over HTTP 1.1 with an
    /// Upgrade to 2.0. Calling this with register this request as running
    /// on stream 1 and returning the response when it is available.
    ///
    /// NOTE: Must be called before 'run()'. The returned future MUST be waited
    /// on after run() though.
    pub async fn receive_upgrade_response(
        &self,
        request_method: Method,
    ) -> Result<impl std::future::Future<Output = Result<Response>> + 'static> {
        let mut connection_state = self.shared.state.lock().await;

        if self.shared.is_server {
            return Err(err_msg("Must be a client to receive a upgrade response"));
        }

        if connection_state.running {
            return Err(err_msg(
                "receive_upgrade_response() called after the connection is running",
            ));
        }

        if connection_state.last_sent_stream_id >= UPGRADE_STREAM_ID {
            return Err(err_msg("Upgrade stream already created?"));
        }

        connection_state.last_sent_stream_id = UPGRADE_STREAM_ID;
        connection_state.local_stream_count += 1;

        let (mut stream, incoming_body, outgoing_body) =
            self.shared.new_stream(&connection_state, UPGRADE_STREAM_ID);

        // Perform a local close.
        {
            let mut stream_state = stream.state.lock().await;
            stream_state.sending_end = true;
            drop(outgoing_body);
            stream.sending_end_flushed = true;
        }

        let (sender, receiver) = new_response_channel();

        // TODO: Deduplicate this code
        let event_sender = self.shared.connection_event_sender.clone();
        let sender = sender.with_cancellation_callback(async move {
            let _ = event_sender
                .send(ConnectionEvent::CancelRequest {
                    stream_id: UPGRADE_STREAM_ID,
                })
                .await;
        });

        stream.incoming_response_handler = Some((request_method, sender, incoming_body));

        connection_state.streams.insert(UPGRADE_STREAM_ID, stream);

        // TODO: Assuming that we sent the right settings, we can assume that the server
        // now knows our settings and we can start using them.

        Ok(receiver.recv())
    }

    /// Called on a server which received a request over HTTP 1.1 with an
    /// Upgrade to 2.0. Calling this will
    ///
    /// NOTE: Must be called before 'run()'
    pub async fn receive_upgrade_request(&self, request: Request) -> Result<()> {
        let mut connection_state = self.shared.state.lock().await;

        // TODO: This could be a convenienct place to deal with reading the settings
        // header?

        // NOTE: Because it isn't running, it likely hasn't gotten into an error state
        // yet.
        if connection_state.running {
            return Err(err_msg(
                "Connection running before upgrade request registered",
            ));
        }

        if !self.shared.is_server {
            return Err(err_msg("Only servers can receive upgrade requests."));
        }

        if connection_state.last_received_stream_id >= UPGRADE_STREAM_ID {
            return Err(err_msg("Multiple upgrade requests received?"));
        }

        connection_state.last_received_stream_id = UPGRADE_STREAM_ID;
        connection_state.remote_stream_count += 1;

        let remote_settings = SettingsContainer::read_from_request(&request.head.headers)?;
        // NOTE: Because we aren't running yet and we haven't created any streams yet,
        // we don't need to do anything special to reconcile our state with the
        // new settings.
        connection_state.remote_settings = remote_settings;
        connection_state.remote_settings_known = true;

        let (mut stream, incoming_body, outgoing_body) =
            self.shared.new_stream(&connection_state, UPGRADE_STREAM_ID);

        // Completely close the remote (client) endpoint.
        {
            let mut stream_state = stream.state.lock().await;
            stream_state.received_end = true;
            drop(incoming_body);
        }

        stream.outgoing_response_handler = Some((request.head.method, outgoing_body));

        stream.processing_tasks.push(ChildTask::spawn(
            self.shared
                .clone()
                .request_handler_driver(UPGRADE_STREAM_ID, request),
        ));

        connection_state.streams.insert(UPGRADE_STREAM_ID, stream);

        Ok(())
    }

    /// Returns whether or not this connection can be used to send additional
    /// client requests without them being locally refused.
    ///
    /// A return value of false implies that the connection is closed or
    /// shutting down. Clients should read check this value before sending a
    /// request and in the case that it is true, then a connection should be
    /// created.
    ///
    /// NOTE: Due to race conditions, immediately calling request() may still
    /// error out instantly with a retryable error so the caller must be
    /// prepared to retry.
    pub async fn accepting_requests(&self) -> bool {
        let connection_state = self.shared.state.lock().await;

        // NOTE: It is not necessary to check upper_sent_stream_id because if that is
        // not MAX_STREAM_ID, then that would imply that we sent or received a
        // GOAWAY which would set this field.
        !connection_state.shutting_down.is_some()
    }

    pub async fn num_outstanding_streams(&self) -> usize {
        let connection_state = self.shared.state.lock().await;
        connection_state.pending_requests.len() + connection_state.streams.len()
    }

    pub async fn enqueue_request(
        &self,
        request: Request,
    ) -> Result<impl Future<Output = Result<Response>>> {
        if request.head.method == Method::CONNECT {
            // Omit :scheme and :path. Only :authority should be added.
            if request.head.uri.authority.is_none()
                || request.head.uri.scheme.is_some()
                || !request.head.uri.path.as_ref().is_empty()
            {
                return Err(err_msg("Invalid CONNECT request"));
            }
        } else {
            if request.head.uri.scheme.is_none() || request.head.uri.path.as_ref().is_empty() {
                return Err(err_msg("Request missing scheme or path"));
            }
        }

        if request.head.uri.fragment.is_some() {
            return Err(err_msg("Can't send path with fragment"));
        }

        // TODO: Double check this.
        if let Some(authority) = &request.head.uri.authority {
            if authority.user.is_some() {
                return Err(err_msg("HTTP2 request not allowed to have user info"));
            }
        }

        // TODO: Somewhere add the Content-Length header. (on both client and server as
        // long as not )

        let (sender, receiver) = new_response_channel();

        // TODO: Fail if the connection runner isn't started yet.

        let empty_queue;
        {
            let mut connection_state = self.shared.state.lock().await;
            if connection_state.shutting_down.is_some() {
                return Err(ProtocolErrorV2 {
                    code: ErrorCode::REFUSED_STREAM,
                    message: "Connection is shutting down",
                    local: true,
                }
                .into());
            }

            // TODO: Ensure this limit isn't hit before the DirectClient marks itself as
            // full.
            if connection_state.pending_requests.len() >= self.shared.options.max_enqueued_requests
            {
                return Err(ProtocolErrorV2 {
                    code: ErrorCode::REFUSED_STREAM,
                    message: "Hit max_enqueued_requests limit on this connection",
                    local: true,
                }
                .into());
            }

            empty_queue = connection_state.pending_requests.is_empty();

            connection_state
                .pending_requests
                .push_front(ConnectionLocalRequest {
                    request,
                    response_sender: sender,
                });
        }

        // For the first request in the queue, send an event so that the
        // connection takes notice
        if empty_queue {
            let _ = self
                .shared
                .connection_event_sender
                .try_send(ConnectionEvent::SendRequest);
        }

        Ok(receiver.recv())
    }

    /// Gets the approximate last time when a byte was sent and received over
    /// the connection.
    pub async fn last_byte_times(&self) -> (std::time::SystemTime, std::time::SystemTime) {
        let state = self.shared.state.lock().await;
        (
            state.last_user_byte_sent_time.clone(),
            state.last_user_byte_received_time.clone(),
        )
    }

    pub async fn ping(&self, data: u64) {
        let _ = self
            .shared
            .connection_event_sender
            .send(ConnectionEvent::Ping {
                ping_frame: PingFramePayload {
                    opaque_data: data.to_le_bytes(),
                },
            })
            .await;
    }

    /// Shuts down the server.
    /// This function will return immediately upon triggering the shutdown with
    /// the actual shutdown occuring later in time (when the run() function
    /// returns).
    ///
    /// NOTE: It is a valid operation to call shutdown on a connection that has
    /// not been starting yet by calling run(). This effectively means that we
    /// prefer shutting down the connection or serving new requests. You should
    /// still call run() if the connection is being shutdown to ensure that we
    /// send the appropriate GOAWAY packets to the other endpoint.
    ///
    /// NOTE: Calling this on an already shutdown connection is a no-op.
    ///
    /// TODO: Need timeouts on the underlying stream if we want to gurantee a
    /// fixed time shutdown when not graceful.
    ///
    /// Arguments:
    /// - graceful: If true, we will wait for all currently active streams to
    ///   close before we shutdown. Otherwise we'll end the connection quickly
    ///   within a fixed amount of time. Even if graceful is set to true,
    ///   shutdown() may be called additional times later with the flag to set
    ///   to false to expedite the shutdown.
    pub async fn shutdown(&self, graceful: bool) {
        Self::shutdown_impl(&self.shared, graceful).await
    }

    /// Shutdown the connection by sending the given error to the remote
    /// connection.
    ///
    /// A non-NO_ERROR code MUST NOT be graceful
    pub async fn shutdown_with_error(&self, graceful: bool, error: ProtocolErrorV2) {
        Self::shutdown_with_error_impl(&self.shared, graceful, error).await
    }

    async fn shutdown_impl(shared: &Arc<ConnectionShared>, graceful: bool) {
        let error = {
            if graceful {
                ProtocolErrorV2 {
                    code: ErrorCode::NO_ERROR,
                    message: "Gracefully shutting down",
                    local: true,
                }
            } else {
                ProtocolErrorV2 {
                    code: ErrorCode::NO_ERROR,
                    message: "About to close connection",
                    local: true,
                }
            }
        };

        Self::shutdown_with_error_impl(shared, graceful, error).await
    }

    async fn shutdown_with_error_impl(
        shared: &Arc<ConnectionShared>,
        graceful: bool,
        error: ProtocolErrorV2,
    ) {
        let mut connection_state = shared.state.lock().await;

        // Ensure that we never decrease the shutdown in severity.
        match &connection_state.shutting_down {
            ShuttingDownState::Complete | ShuttingDownState::Abrupt => {
                // No need to do anything.
                return;
            }
            ShuttingDownState::Graceful { .. } => {
                // No point in doing a Graceful shutdown while one is already in process.
                if graceful {
                    return;
                }
            }
            ShuttingDownState::No | ShuttingDownState::GracefulRemote => {
                // We can do either type of shutdown.
            }
        };

        if graceful {
            // We'll keep the upper_received_stream_id at MAX_STREAM_ID and just send a
            // GOAWAY.

            if shared.is_server {
                // We won't make any changes to the upper_received_stream_id so
                // that in-flight but unreceived client requests
                // can still be processed.
            } else {
                connection_state.upper_received_stream_id =
                    connection_state.last_received_stream_id;
            }

            let timeout_task = ChildTask::spawn(Self::wait_shutdown_timeout(shared.clone()));

            connection_state
                .set_shutting_down(ShuttingDownState::Graceful {
                    timeout_task: Some(timeout_task),
                })
                .await;

            let _ = shared
                .connection_event_sender
                .send(ConnectionEvent::Closing {
                    send_goaway: Some(error),
                    close_with: None,
                })
                .await;
        } else {
            // Escalate to an abrupt error by setting the upper_received_stream_id and
            // sending a Closing/GOAWAY.

            connection_state.upper_received_stream_id = connection_state.last_received_stream_id;
            connection_state
                .set_shutting_down(ShuttingDownState::Abrupt)
                .await;

            //
            let _ = shared
                .connection_event_sender
                .send(ConnectionEvent::Closing {
                    send_goaway: Some(error),
                    close_with: Some(Ok(())),
                })
                .await;
        }

        // TODO: We should also immediately cancel anything in
        // 'pending_requests'
    }

    fn wait_shutdown_timeout(
        shared: Arc<ConnectionShared>,
    ) -> impl std::future::Future<Output = ()> + Send + 'static {
        async move {
            executor::sleep(shared.options.graceful_shutdown_timeout.clone()).await;

            let _ = Self::shutdown_impl(&shared, false).await;
        }
    }

    /// Runs the connection management threads.
    /// This must be called exactly once and continously polled to keep the
    /// connection alive.
    ///
    /// NOTE: The return value of this MUST be continously polled until
    /// completetion even if you are done sending requests to the connection
    /// to ensure that any outstanding requests/responses are completed. If
    /// we want this to stop early, then you should trigger a shutdown()
    ///
    /// This function will return once the connection has been terminated. This
    /// could be either because:
    /// - A fatal connection level error was locally/remotely generated (the
    ///   error is returned in the result)
    /// - The connection was gracefully shut down
    /// If an unexpected connection level error occurs, it will be returned from
    pub fn run(
        &self,
        initial_state: ConnectionInitialState,
        reader: Box<dyn Readable>,
        writer: Box<dyn Writeable>,
    ) -> impl std::future::Future<Output = Result<()>> {
        Self::run_inner(self.shared.clone(), initial_state, reader, writer)
    }

    async fn run_inner(
        shared: Arc<ConnectionShared>,
        initial_state: ConnectionInitialState,
        reader: Box<dyn Readable>,
        writer: Box<dyn Writeable>,
    ) -> Result<()> {
        {
            let mut state = shared.state.lock().await;

            if state.running {
                return Err(err_msg("run() can only be called once per connection"));
            }
            state.running = true;
        }

        // NOTE: We could use a select! for these, but we'd rather run them in separate
        // tasks so that they can run in separate CPU threads.
        let reader_task = executor::spawn(
            ConnectionReader::new(shared.clone()).run(reader, initial_state.seen_preface_head),
        );

        let mut result = ConnectionWriter::new(shared.clone())
            .run(writer, initial_state.upgrade_payload)
            .await;

        let _ = reader_task.cancel().await;

        // TODO: If all of our streams aren't closed, we should definately provide some
        // form of error.
        /*
        Error priority is:
        1. Check for a remote GOAWAY
        2. Check for IO error
        3. Check if there are some streams (maybe excluding pushes) which are still active.
        */

        // TODO: Prioritize returning any error received from the ConnectionReader in a
        // GOAWAY packet (may still be in the event channel but not yet processed by the
        // writer).

        let mut connection_state = shared.state.lock().await;

        // Well behaved peers SHOULD send a GOAWAY before closing the connection so
        // allow ignoring abrupt pipe closures in this case.
        //
        // TODO: Should we
        // also verify that all streams have been processed.
        if let Err(e) = &result {
            // TODO: Annoying part of this logic is that we can't run HTTP2 on HTTP2
            // (because HTTP2 generates ProtocolErrorV2 errors which )
            if let Some(io_error) = e.downcast_ref::<IoError>() {
                let got_remote_goaway = match &connection_state.shutting_down {
                    ShuttingDownState::GracefulRemote => true,
                    _ => false,
                };

                if got_remote_goaway {
                    result = Ok(());
                }
            }
        }

        if result.is_ok() && !connection_state.streams.is_empty() {
            result = Err(IoError::new(
                IoErrorKind::Aborted,
                "HTTP2 Connection closed while streams are still active",
            )
            .into());
        }

        // Cleanup all outstanding state.
        // TODO: Ideally if we did everything correctly then this shouldn't be needed
        // right?
        {
            connection_state
                .set_shutting_down(ShuttingDownState::Complete)
                .await;
            // TODO: Hopefully this line is not needed?
            connection_state.upper_received_stream_id = connection_state.last_received_stream_id;

            // TODO: Should we call finish_stream to perform this cleanup?
            for (stream_id, stream) in connection_state.streams.iter_mut() {
                if let Some((_, response_sender, _)) = stream.incoming_response_handler.take() {
                    // TODO: Check if this is a good error to return.
                    response_sender
                        .send(Err(ProtocolErrorV2 {
                            code: ErrorCode::STREAM_CLOSED,
                            message: "Connection shutting down.",
                            local: true,
                        }
                        .into()))
                        .await;
                }
            }
            connection_state.streams.clear();

            while let Some(req) = connection_state.pending_requests.pop_front() {
                req.response_sender
                    .send(Err(ProtocolErrorV2 {
                        code: ErrorCode::REFUSED_STREAM,
                        message: "Connection shutting down",
                        local: true,
                    }
                    .into()))
                    .await;
            }
        }

        // TODO: No matter what, go through the state and verify that every pending
        // request is refused gracefully. Any streams that are still active
        // should also be cleaned out.

        // TODO: If the write thread failed, we probably need to cleanup the streams,
        // mark the connection is errored out and probably also kill any pending
        // requests.

        result
    }
}

/*
    Error handling:
    - If the reader encounters a stream error:
        - Delete the stream
        - Send a message to the writer to trigger a RST_STREAM
    - If the reader encounters a connection error:
        - Tell the writer that the connection is busted.
        - Immediately bail out.
        => In response the writer can send the GOAWAY and attempt to finish writing responses to any remotely initialized requests
*/

/*

When generating requests:
- In HTTP 1.1:
    - Must always be sending a Host header
    - Prefer not to send the authority form of the request-target
- In HTTP 2:
    - Generate :authority only

When receiving requests:
- If HTTP 1.1:
    - Prefer to use the request target's authority if available.
    - Otherwise, read the Host header
- In HTTP 2
    - Can always rely in the ':authority', ignore any 'Host' header given

*/

/*
TODO:

 To ensure that the HTTP/1.1 request line can be reproduced
      accurately, this pseudo-header field MUST be omitted when
      translating from an HTTP/1.1 request that has a request target in
      origin or asterisk form (see [RFC7230], Section 5.3).
*/

#[cfg(test)]
mod tests {
    use super::*;

    use common::pipe::pipe;
    use net::ip::IPAddress;

    use crate::body::{BodyFromData, EmptyBody};
    use crate::method::Method;
    use crate::request::{Request, RequestBuilder};
    use crate::response::{Response, ResponseBuilder};
    use crate::server_handler::ServerConnectionContext;
    use crate::server_handler::ServerRequestContext;
    use crate::status_code;

    /// Simple request handler which performs various numerical calculations.
    struct CalculatorServerHandler {}

    #[async_trait]
    impl ServerHandler for CalculatorServerHandler {
        async fn handle_request<'a>(
            &self,
            request: Request,
            context: ServerRequestContext<'a>,
        ) -> Response {
            println!("GOT REQUEST: {:?}", request.head);

            ResponseBuilder::new()
                .status(crate::status_code::OK)
                .body(crate::body::EmptyBody())
                .build()
                .unwrap()
        }
    }

    #[testcase]
    async fn connection_test() -> Result<()> {
        let (writer1, reader1) = pipe();
        let (writer2, reader2) = pipe();

        let options = ConnectionOptions::default();

        let server_options = ServerConnectionOptions {
            connection_context: ServerConnectionContext {
                id: 0,
                peer_addr: IPAddress::V4([0, 0, 0, 0]),
                peer_port: 0,
                tls: None,
            },
            request_handler: Box::new(CalculatorServerHandler {}),
        };

        let server_conn = Connection::new(options.clone(), Some(server_options));
        let server_task = executor::spawn(server_conn.run(
            ConnectionInitialState::raw(),
            Box::new(reader1),
            Box::new(writer2),
        ));

        let client_conn = Connection::new(options, None);
        let client_task = executor::spawn(client_conn.run(
            ConnectionInitialState::raw(),
            Box::new(reader2),
            Box::new(writer1),
        ));

        let res = client_conn
            .enqueue_request(
                RequestBuilder::new()
                    .method(Method::GET)
                    .uri("http://localhost/hello")
                    .build()
                    .unwrap(),
            )
            .await?
            .await?;

        println!("{:?}", res.head);

        Ok(())
    }

    #[testcase]
    async fn connect_client_closing() -> Result<()> {
        let (writer1, reader1) = pipe();
        let (writer2, reader2) = pipe();

        let options = ConnectionOptions::default();

        let client_conn = Connection::new(options, None);
        let client_task = executor::spawn(client_conn.run(
            ConnectionInitialState::raw(),
            Box::new(reader2),
            Box::new(writer1),
        ));

        drop(reader1);
        drop(writer2);

        let f = client_conn
            .enqueue_request(
                RequestBuilder::new()
                    .method(Method::GET)
                    .uri("http://localhost/hello")
                    .build()
                    .unwrap(),
            )
            .await;

        let res = match f {
            Ok(v) => v.await,
            Err(e) => Err(e),
        };

        assert!(res.is_err());

        Ok(())
    }

    /*
    Test cases to write:
    - Send request with empty body to server and receive returned body.
    - Send request with empty body and receive empty body.
    - reader_closed

    - Send a request after a first request.
    - Send two requests at the same time from a client to the server.
        - Ensure server correctly muxes them.

    - Test sending and receiving a very large body requiring flow control.
        - Send 10MB

    - If we send a request or request a response with a Content-Length that doesn't match the actual length of the stream, we should return an error.

    - Test that if out outgoing body has a well defined length, then we add a Content-Length header automatically to the request.

    - Test that if we send an empty body with no trailes,

    - Test that when using the DirectClient, we can't provide reserved headers like Content-Length

    - Test that if we send headers that are too long, then we don't break the connection (only the stream)

    - If we exist

    - Test that even if we stop reading an incoming body, the connection still keeps track of ensuring the remaining data is the correct length (this implies that we are still writing to the stream as otherwise we can just drop the stream entirely).

    TODO: Enforce a timeout on packets received on dead streams.
    - If we receive a packet on a dead stream more than 2 seconds after it was killed, let's just end the connection.
    */
}
