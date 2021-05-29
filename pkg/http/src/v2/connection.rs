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
use crate::v2::connection_shared::ConnectionShared;
use crate::v2::connection_reader::ConnectionReader;
use crate::v2::connection_writer::ConnectionWriter;


const FLOW_CONTROL_MAX_SIZE: WindowSize = ((1u32 << 1) - 1) as i32;

const MAX_STREAM_ID: StreamId = (1 << 31) - 1;

/// Maximum number of bytes per stream that we will allow to be enqueued for sending to the
/// remote server.
///
/// The actual max used will be the min of this value of the remote flow control window
/// size. We maintain this as a separate setting to ensure that a misbehaving remote endpoint
/// can't force us to use large amounts of memory while queuing data. 
const MAX_SENDING_BUFFER_SIZE: usize = 1 << 16;  // 64 KB

/// 
const MAX_ENCODER_TABLE_SIZE: usize = 8192;

/// NOTE: This is a client stream id. 
const UPGRADE_STREAM_ID: StreamId = 1;

/// NOTE: The connection frame control window is only updated on WINDOW_UPDATE frames (not SETTINGS)
const INITIAL_CONNECTION_WINDOW_SIZE: WindowSize = 65535;

// TODO: Should also use PING to countinuously verify that the server is still alive.
//
//  Received a GOAWAY with error code ENHANCE_YOUR_CALM and debug data equal to "too_many_pings"
// https://github.com/grpc/grpc/blob/fd3bd70939fb4239639fbd26143ec416366e4157/doc/keepalive.md
//
// 

// 6.9.3.

/*
#[derive(PartialEq, Debug)]
enum StreamState {
    Idle,
    Open,
    ReservedLocal,
    ReservedRemote,

    /// The local endpoint is no longer sending data on the stream. There may still be remote
    /// data available for reading.
    HalfClosedLocal,

    HalfClosedRemote,

    Closed
}
*/


/*
    Eventually we want to have a HTTP2 specific wrapper around a Request/Response to support
    changing settings, assessing stream/connection ids, or using the push functionality.
*/

pub struct ConnectionOptions {
    pub protocol_settings: SettingsContainer,

    pub max_sending_buffer_size: usize,

    pub max_local_encoder_table_size: usize,

    pub settings_ack_timeout: Duration,

    /// Maximum number of locally initialized streams
    /// The actual number used will be:
    /// 'min(max_outgoing_stream, remote_settings.MAX_CONCURRENT_STREAMS)'
    pub max_outgoing_streams: usize
}


/// Describes any past processing which has already happened on the connection
/// before it was handed to the HTTP2 'Connection' for further processing.
pub struct ConnectionInitialState {
    /// This is an HTTP server and we've already read the first line of the HTTP 2.0 preface
    /// from the client. The second half of the preface still needs to be read.
    ///
    /// This is a convenience feature that is for enabling the easy implementation of HTTP 2
    /// on top of an existing HTTP 1 server which scans the request head and then upgrades
    /// if seeing an HTTP 2 version.
    pub seen_preface_head: bool,

    /// We are upgrading using an HTTP 1.1 request/response.
    /// Usually this requires that some remaining data is written out to the stream before
    /// it can be used for HTTP 2. (e.g. the HTTP 1.1 request body or the HTTP 1.1 101 upgrade
    /// response). To support these requirements, this data can be passed in this state and
    /// the HTTP2 connection will ensure that this data is written prior to HTTP2 data.
    pub upgrade_payload: Option<Box<dyn Readable>>,
}

impl ConnectionInitialState {
    pub fn raw() -> Self {
        Self { seen_preface_head: false, upgrade_payload: None }
    }
}


// TODO: Make sure we signal a small enough value to the hpack encoder to be reasonable.

// TODO: Make sure we reject new streams when in a goaway state.

// TODO: Should we support allowing the connection itself to stay half open.

/// A single HTTP2 connection to a remote endpoint.
pub struct Connection {
    shared: Arc<ConnectionShared>
}

impl Connection {
    /*
        Based on 8.1.2.3, Request must contain :method, :scheme, :path unless it is a CONNECT request (8.3)
        
        May contain ':authority'

        for OPTIONS, :path should be '*' instead of empty.
    */


    pub fn new(server_request_handler: Option<Box<dyn RequestHandler>>) -> Self {
        let local_settings = SettingsContainer::default();
        let remote_settings = SettingsContainer::default();
        let is_server = server_request_handler.is_some();

        // TODO: Implement SETTINGS_MAX_HEADER_LIST_SIZE.

        Connection {
            shared: Arc::new(ConnectionShared {
                is_server,
                request_handler: server_request_handler,
                connection_event_channel: channel::unbounded(),
                state: Mutex::new(ConnectionState {
                    running: false,
                    error: None,
                    remote_header_decoder: hpack::Decoder::new(
                        local_settings[SettingId::HEADER_TABLE_SIZE] as usize),
                    local_settings: local_settings.clone(),
                    local_settings_ack_waiter: None,
                    local_pending_settings: local_settings.clone(),
                    local_connection_window: INITIAL_CONNECTION_WINDOW_SIZE,
                    remote_settings: remote_settings.clone(),
                    remote_settings_known: false,
                    remote_connection_window: INITIAL_CONNECTION_WINDOW_SIZE,
                    last_received_stream_id: 0,
                    last_sent_stream_id: 0,
                    pending_requests: std::collections::VecDeque::new(),
                    local_stream_count: 0,
                    remote_stream_count: 0,

                    streams: HashMap::new()
                })
            })
        }
    }

    /// Called on a client which just sent a request over HTTP 1.1 with an Upgrade to 2.0.
    /// Calling this with register this request as running on stream 1 and returning the response
    /// when it is available.
    ///
    /// NOTE: Must be called before 'run()'. The returned future MUST be waited on after run() though.
    pub async fn receive_upgrade_response(&self, request_method: Method)
    -> Result<impl std::future::Future<Output=Result<Response>>> {
        let mut connection_state = self.shared.state.lock().await;

        if self.shared.is_server {
            return Err(err_msg("Must be a client to receive a upgrade response"));
        }

        if connection_state.running {
            return Err(err_msg("receive_upgrade_response() called after the connection is running"));
        }

        if connection_state.last_sent_stream_id >= UPGRADE_STREAM_ID {
            return Err(err_msg("Upgrade stream already created?"))
        }

        connection_state.last_sent_stream_id = UPGRADE_STREAM_ID;
        connection_state.local_stream_count += 1;

        let (mut stream, incoming_body, outgoing_body) = self.shared.new_stream(
            &connection_state, UPGRADE_STREAM_ID);

        // Perform a local close.
        {
            let mut stream_state = stream.state.lock().await;
            stream_state.sending_at_end = true;
            drop(outgoing_body);
            stream.sent_end_of_stream = true;
        }


        let (sender, receiver) = channel::bounded::<Result<Response>>(1);

        stream.incoming_response_handler = Some((request_method, Box::new(sender), incoming_body));

        connection_state.streams.insert(UPGRADE_STREAM_ID, stream);
        

        // TODO: Assuming that we sent the right settings, we can assume that the server now knows 
        // our settings and we can start using them.

        Ok(Self::receiver_future(receiver))
    }

    async fn receiver_future(receiver: channel::Receiver<Result<Response>>) -> Result<Response> {
        receiver.recv().await?
    }

    /// Called on a server which received a request over HTTP 1.1 with an Upgrade to 2.0.
    /// Calling this will
    ///
    /// NOTE: Must be called before 'run()'
    pub async fn process_upgrade_request(&self, request: Request) -> Result<()> {
        let mut connection_state = self.shared.state.lock().await;

        // TODO: This could be a convenienct place to deal with reading the settings header?

        // NOTE: Because it isn't running, it likely hasn't gotten into an error state yet.
        if connection_state.running {
            return Err(err_msg("Connection running before upgrade request registered"));
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
        // NOTE: Because we aren't running yet and we haven't created any streams yet, we don't need to do
        // anything special to reconcile our state with the new settings.
        connection_state.remote_settings = remote_settings;
        connection_state.remote_settings_known = true;


        let (mut stream, incoming_body, outgoing_body) = self.shared.new_stream(
            &connection_state,  UPGRADE_STREAM_ID);

        // Completely close the remote (client) endpoint. 
        {
            let mut stream_state = stream.state.lock().await;
            stream_state.received_end_of_stream = true;
            drop(incoming_body);
        }

        stream.outgoing_response_handler = Some(outgoing_body);

        stream.processing_tasks.push(ChildTask::spawn(self.shared.clone().request_handler_driver(
            UPGRADE_STREAM_ID, request)));

        connection_state.streams.insert(UPGRADE_STREAM_ID, stream);

        Ok(())
    }


    pub async fn request(&self, request: Request) -> Result<Response> {
        if request.head.method == Method::CONNECT {
            // Omit :scheme and :path. Only :authority should be added.
            if request.head.uri.authority.is_none() || request.head.uri.scheme.is_some() ||
                !request.head.uri.path.as_ref().is_empty() {
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
        
        let (sender, receiver) = channel::bounded::<Result<Response>>(1);


        let empty_queue;
        {
            let mut connection_state = self.shared.state.lock().await;
            if connection_state.error.is_some() {
                return Err(ProtocolErrorV2 {
                    code: ErrorCode::REFUSED_STREAM,
                    message: "Connection is shutting down",
                    local: true
                }.into());
            }

            empty_queue = connection_state.pending_requests.is_empty();

            connection_state.pending_requests.push_front(ConnectionLocalRequest {
                request,
                response_handler: Box::new(sender)
            });
        }

        // For the first request in the queue, send an event so that the
        // connection takes notice
        if empty_queue {
            let _ = self.shared.connection_event_channel.0.try_send(ConnectionEvent::SendRequest);
        }
        
        // TODO: If the receiver fails, does that mean that we can definately retry the request?
        receiver.recv().await?
    }

    /// Shuts down the server.
    /// This function will return immediately upon triggering the shutdown with the actual
    /// shutdown occuring later in time (when the run() function returns).
    ///
    /// NOTE: Calling this on an already shutdown connection is a no-op.
    ///
    /// TODO: Need timeouts on the underlying stream if we want to gurantee a fixed time shutdown
    /// when not graceful.
    ///
    /// Arguments:
    /// - graceful: If true, we will wait for all currently active streams to close before
    ///             we shutdown. Otherwise we'll end the connection quickly within a fixed
    ///             amount of time. Even if graceful is set to true, shutdown() may be called
    ///             additional times later with the flag to set to false to expedite the shutdown.
    pub async fn shutdown(&self, graceful: bool) -> Result<()> {
        
        // Need to set an error

        Err(err_msg("Shutting down"))
    }

    // TODO: Need to support initializing with settings already negiotated via HTTP

    // TODO: Verify that run is never called more than once on the same Connection instance.

    /// Runs the connection management threads.
    /// This must be called exactly once and continously polled to keep the connection alive.
    ///
    /// This function will return once the connection has been terminated. This could be either because:
    /// - A fatal connection level error was locally/remotely generated (the error is returned in the result)
    /// - The connection was gracefully shut down
    /// If an unexpected connection level error occurs, it will be returned from 
    ///
    pub fn run(
        &self, initial_state: ConnectionInitialState, reader: Box<dyn Readable>, writer: Box<dyn Writeable>
    ) -> impl std::future::Future<Output=Result<()>> {
        Self::run_inner(self.shared.clone(), initial_state, reader, writer)
    }

    async fn run_inner(
        shared: Arc<ConnectionShared>, initial_state: ConnectionInitialState,
        reader: Box<dyn Readable>, writer: Box<dyn Writeable>
    ) -> Result<()> {
        {
            let mut state = shared.state.lock().await;

            if state.running {
                return Err(err_msg("run() can only be called once per connection"));
            }
            state.running = true;
        }

        // NOTE: We could use a select! for these, but we'd rather run them in separate tasks so that they
        // can run in separate CPU threads.
        let reader_task = task::spawn(ConnectionReader::new(shared.clone()).run(
            reader, initial_state.seen_preface_head));

        let result = ConnectionWriter::new(shared).run(writer, initial_state.upgrade_payload).await;
        if !result.is_ok() {
            println!("HTTP2 WRITE THREAD FAILED: {:?}", result);
        }

        let _ = reader_task.cancel().await;

        // TODO: If the write thread failed, we probably need to cleanup the streams, mark the connection is errored out
        // and probably also kill any pending requests.

        result
        
        // TODO: Ensure that the first set of settings are acknowledged.

        // Write settings frame
        // TODO: If the settings frame contains parameters with default values, don't send them.

        // Wait for first settings frame from remote endpoint if we haven't already figured out the remote
        // endpoint's settings.

        // Let's say we get a Request, what do we do?
        // - Get a new stream/id
        // - begin sending the headers is a contigous chunk
        // - Set stream is Open and start sending 
        // - Start a new thread to read from the body into a buffer. 

        // Depending on the 

        // TODO: While sending/receiving headers, we should still be able to receive/send on the other half of the pipe.

    }

    // TODO: According to RFC 7540 Section 4.1, undefined flags should be left as zeros.
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

    use crate::request::{Request, RequestBuilder};
    use crate::response::{Response, ResponseBuilder};
    use crate::body::{BodyFromData, EmptyBody};
    use crate::status_code;
    use crate::method::Method;

    /// Simple request handler which performs various numerical calculations.
    struct CalculatorRequestHandler {}

    #[async_trait]
    impl RequestHandler for CalculatorRequestHandler {
        async fn handle_request(&self, request: Request) -> Response {
            println!("GOT REQUEST: {:?}", request.head);

            ResponseBuilder::new()
                .status(crate::status_code::OK)
                .body(crate::body::EmptyBody())
                .build().unwrap()
        }
    }


    #[async_std::test]
    async fn connection_test() -> Result<()> {
        let (writer1, reader1) = pipe();
        let (writer2, reader2) = pipe();

        let server_conn = Connection::new(
            Some(Box::new(CalculatorRequestHandler {})));
        let server_task = task::spawn(server_conn.run(
            ConnectionInitialState::raw(), Box::new(reader1), Box::new(writer2)));

        let client_conn = Connection::new(None);
        let client_task = task::spawn(client_conn.run(
            ConnectionInitialState::raw(),Box::new(reader2), Box::new(writer1)));

        let res = client_conn.request(RequestBuilder::new()
            .method(Method::GET)
            .uri("http://localhost/hello")
            .body(EmptyBody())
            .build()
            .unwrap()).await?;

        println!("{:?}", res.head);

        Ok(())
    }

}
