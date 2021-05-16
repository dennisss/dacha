use std::{convert::TryFrom, sync::Arc};
use std::collections::HashMap;

use common::{chrono::Duration, errors::*};
use common::io::{Writeable, Readable};
use common::async_std::channel;
use common::async_std::sync::Mutex;
use common::async_std::task;
use common::futures::select;
use common::chrono::prelude::*;
use parsing::ascii::AsciiString;

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

const CONNECTION_PREFACE: &[u8] = b"PRI * HTTP/2.0\r\n\r\nSM\r\n\r\n";

const CONNECTION_PREFACE_BODY: &[u8] = b"SM\r\n\r\n";

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

/// Amount of time after which we'll close the connection if we don't receive an acknowledment to our
/// 
const SETTINGS_ACK_TIMEOUT_SECS: usize = 10;

const UPGRADE_STREAM_ID: StreamId = 1;

// NOTE: The connection frame control window is only updated on WINDOW_UPDATE frames (not SETTINGS)
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

enum ReceivedHeadersType {
    PushPromise {
        promised_stream_id: StreamId,
    },
    RegularHeaders {
        end_stream: bool,
        priority: Option<PriorityFramePayload>
    }
}

struct ReceivedHeaders {
    /// Id of the stream on which this data was received.
    stream_id: StreamId,

    data: Vec<u8>,

    typ: ReceivedHeadersType
}

pub struct ConnectionOptions {
    pub protocol_settings: SettingsContainer,

    pub max_sending_buffer_size: usize,

    pub max_local_encoder_table_size: usize,

    pub settings_ack_timeout: Duration
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
///
/// NOTE: This is not a complete HTTP Client/Server interface as it is mainly focused
/// on implementing the protocol details and doesn't handle transfer level details like
/// Content-Length or Transfer-Encoding, etc. 
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
                    local_settings_sent_time: None,
                    local_pending_settings: local_settings.clone(),
                    local_connection_window: INITIAL_CONNECTION_WINDOW_SIZE,
                    // local_connection_window: local_settings[SettingId::INITIAL_WINDOW_SIZE] as WindowSize,
                    remote_settings: remote_settings.clone(),
                    remote_settings_known: false,
                    remote_connection_window: INITIAL_CONNECTION_WINDOW_SIZE,
                    // remote_connection_window: remote_settings[SettingId::INITIAL_WINDOW_SIZE] as WindowSize,
                    last_received_stream_id: 0,
                    last_sent_stream_id: 0,
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
    pub async fn receive_upgrade_response(&self) -> Result<impl std::future::Future<Output=Result<Response>>> {
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

        let (mut stream, incoming_body, outgoing_body) = self.shared.new_stream(
            &connection_state, UPGRADE_STREAM_ID);

        // Perform a local close.
        {
            let mut stream_state = stream.state.lock().await;
            stream_state.sending_at_end = true;
            drop(outgoing_body);
        }


        let (sender, receiver) = channel::bounded::<Result<Response>>(1);

        stream.incoming_response_handler = Some((Box::new(sender), incoming_body));

        connection_state.streams.insert(UPGRADE_STREAM_ID, stream);

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

        stream.processing_tasks.push(task::spawn(self.shared.clone().request_handler_driver(
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

        // TODO: Check if the connection is still alive before shipping out the request.

        self.shared.connection_event_channel.0.send(ConnectionEvent::SendRequest {
            request,
            response_handler: Box::new(sender)
        }).await;
        
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
        self.shared.clone().run(initial_state, reader, writer)
    }

    
}

struct ConnectionShared {
    is_server: bool,

    state: Mutex<ConnectionState>,

    // TODO: We may want to keep around a timer for the last time we closed a stream so that if we 

    /// Handler for producing responses to incoming requests.
    ///
    /// NOTE: This will only be used in HTTP servers.
    request_handler: Option<Box<dyn RequestHandler>>,

    /// Used to notify the connection of events that have occured.
    /// The writer thread listens to these events performs actions such as sending more data, starting
    /// requests, etc. in response to each event.
    ///
    /// TODO: Make this a bounded channel?
    connection_event_channel: (channel::Sender<ConnectionEvent>, channel::Receiver<ConnectionEvent>),

    // remote_settings_known_channel: (channel::Sender<ConnectionEvent>, )

    // Stream ids can't be re-used.
    // Also, stream ids can't be 
}

impl ConnectionShared {
    async fn run(
        self: Arc<Self>, initial_state: ConnectionInitialState,
        reader: Box<dyn Readable>, writer: Box<dyn Writeable>
    ) -> Result<()> {
        {
            let mut state = self.state.lock().await;

            if state.running {
                return Err(err_msg("run() can only be called once per connection"));
            }
            state.running = true;
        }

        // NOTE: We could use a select! for these, but we'd rather run them in separate tasks so that they
        // can run in separate CPU threads.
        let reader_task = task::spawn(self.clone().run_read_thread(
            reader, initial_state.seen_preface_head));

        let result = self.run_write_thread(writer, initial_state.upgrade_payload).await;
        println!("HTTP2 WRITE THREAD FAILED: {:?}", result);

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

    async fn recv_reset_stream(&self, stream_id: StreamId, error: ProtocolErrorV2) -> Result<()> {
        let mut connection_state = self.state.lock().await;

        let mut stream = {
            if let Some(stream) = connection_state.streams.remove(&stream_id) {
                stream
            } else {
                // Ignore requests for already closed streams.
                return Ok(())
            }
        };

        let mut stream_state = stream.state.lock().await;
        stream_state.error = Some(error.clone());

        // TODO: Whenever we are resetting a stream, we can allow the other stream to 

        // If there is unread data when resetting a stream we can clear it and count it as 'read' by
        // increasing the connection level flow control limit. 
        if stream_state.received_buffer.len() > 0 {
            self.connection_event_channel.0.send(ConnectionEvent::StreamRead {
                stream_id: 0,
                count: stream_state.received_buffer.len()
            }).await;
        }

        // Clear no longer needed memory.
        // TODO: Make sure that this doesn't happen if we are just gracefully closing a stream.
        stream_state.received_buffer.clear();
        stream_state.sending_buffer.clear();

        while let Some(handle) = stream.processing_tasks.pop() {
            // TODO: Do I need to task::spawn() this?
            handle.cancel();
        }

        // If the error happened before response headers will received by a client, response with an error.
        // TODO: Also need to notify the requester of whether or not the request is trivially retryable
        // (based on the stream id in the latest GOAWAY message).
        if let Some((response_handler, body)) = stream.incoming_response_handler.take() {
            response_handler.handle_response(Err(error.into()));
        }

        if let Some(outgoing_body) = stream.outgoing_response_handler.take() {
            // I don't think I need to do anything here?
        }

        // Notify all reader/writer threads that the stream is dead.
        // TODO: Handle errors on all these things.
        stream.read_available_notifier.send(()).await;
        stream.write_available_notifier.send(()).await;

        Ok(())
    }

    async fn send_reset_stream(&self, stream_id: StreamId, error: ProtocolErrorV2) -> Result<()> {
        self.recv_reset_stream(stream_id, error.clone());
        
        // Notify the writer thread that to let the other endpoint know that the stream should be killed.
        self.connection_event_channel.0.send(ConnectionEvent::ResetStream { stream_id, error }).await;

        Ok(())
    }

    fn is_local_stream_id(&self, id: StreamId) -> bool {
        // Clients have ODD numbered ids. Servers have EVEN numbered ids.
        self.is_server == (id % 2 == 0)
    }

    fn is_remote_stream_id(&self, id: StreamId) -> bool {
        !self.is_local_stream_id(id)
    }

    // TODO: According to 8.1.2.1, if a headers blockis received with regular headers before pseudo headers
    // is malformed (stream error PROTOCOL_ERROR)


    /// Runs the thread that is the exlusive reader of incoming data from the raw connection.
    ///
    /// Internal Error handling:
    /// - If a connection error occurs, this function will return immediately with a non-ok result.
    ///   The caller should communicate this to the 
    ///
    /// External Error Handling:
    /// - The caller should cancel this future when it wants to 
    async fn run_read_thread(self: Arc<Self>, reader: Box<dyn Readable>,
                             skip_preface_head: bool) {
        let result = self.run_read_thread_inner(reader, skip_preface_head).await;
        
        match result {
            Ok(()) => {
                let _ = self.connection_event_channel.0.send(ConnectionEvent::Closing).await;
            }
            Err(e) => {
                // TODO: Improve reporting of these errors up the call chain.
                println!("HTTP2 READ THREAD FAILED: {:?}", e);

                let proto_error = if let Some(e) = e.downcast_ref::<ProtocolErrorV2>() {
                    // We don't need to send a GOAWAY from remotely generated errors.
                    if !e.local {
                        let _ = self.connection_event_channel.0.send(ConnectionEvent::Closing).await;
                        return;
                    }

                    e.clone()
                } else {
                    ProtocolErrorV2 {
                        code: ErrorCode::INTERNAL_ERROR,
                        message: "Unknown internal error occured",
                        local: true
                    }
                };

                let last_stream_id = {
                    let connection_state = self.state.lock().await;
                    connection_state.last_received_stream_id
                };

                let _ = self.connection_event_channel.0.send(ConnectionEvent::Goaway {
                    error: proto_error,
                    last_stream_id
                }).await;
            }
        }
    }

    // NOTE: Will return an Ok(()) if and only if the 
    async fn run_read_thread_inner(self: &Arc<Self>, mut reader: Box<dyn Readable>,
                                   seen_preface_head: bool) -> Result<()> {
        if self.is_server {
            let preface_str = if seen_preface_head { CONNECTION_PREFACE_BODY } else { CONNECTION_PREFACE };

            let mut preface = [0u8; CONNECTION_PREFACE.len()];
            reader.read_exact(&mut preface[0..preface_str.len()]).await?;
            if &preface[0..preface_str.len()] != preface_str {
                return Err(err_msg("Incorrect preface received"));
            }
        }

        // TODO: Don't allow starting any new streams until we've gotten the remote settings 

        // If the read thread fails, we should tell the write thread to complain about an error.
        // Likewise we need to be able to send other types of events to the write thread.

        // TODO: Receiving any packet on a stream with a smaller number than any stream id ever seen
        // should casue an error.

        // Promised request:
        // (promised_stream_id)

        let mut pending_header: Option<ReceivedHeaders> = None;

        let mut frame_header_buf = [0u8; FrameHeader::size_of()];
        loop {
            reader.read_exact(&mut frame_header_buf).await?;

            let header_frame = FrameHeader::parse_complete(&frame_header_buf)?;

            println!("Got frame: {:?}", header_frame.typ);

            let max_frame_size = {
                let state = self.state.lock().await;
                state.local_settings[SettingId::MAX_FRAME_SIZE]
            };

            // TODO: first frame must always be the Settings frame 
            

            // Error handling based on RFC 7540: Section 4.2
            if header_frame.length > max_frame_size {
                let can_alter_state =
                    header_frame.typ == FrameType::SETTINGS ||
                    header_frame.typ == FrameType::HEADERS ||
                    header_frame.typ == FrameType::PUSH_PROMISE ||
                    header_frame.typ == FrameType::CONTINUATION ||
                    header_frame.stream_id == 0;
                
                if can_alter_state {
                    return Err(ProtocolErrorV2 {
                        code: ErrorCode::FRAME_SIZE_ERROR,
                        message: "Received state altering frame larger than max frame size",
                        local: true
                    }.into());
                } else {
                    self.send_reset_stream(header_frame.stream_id, ProtocolErrorV2 {
                        code: ErrorCode::FRAME_SIZE_ERROR,
                        message: "Received frame larger than max frame size",
                        local: true
                    }).await?;
                }

                // Skip over this frame's payload by just reading into a waste buffer until we reach
                // the end of the packet.
                let mut num_remaining = header_frame.length as usize;
                while num_remaining > 0 {
                    let mut buf = [0u8; 512];

                    let max_to_read = std::cmp::min(num_remaining, buf.len());
                    let n = reader.read(&mut buf[0..max_to_read]).await?;
                    num_remaining -= n;

                    if n == 0 {
                        return Ok(());
                    }
                }

                continue;
            }

            if let Some(received_header) = &pending_header {
                if header_frame.stream_id == received_header.stream_id ||
                   header_frame.typ != FrameType::CONTINUATION {
                    // TODO: Verify that this is the right error code.
                    return Err(ProtocolErrorV2 {
                        code: ErrorCode::PROTOCOL_ERROR,
                        message: "",
                        local: true
                    }.into());    
                }
            }

            // TODO: Read this on demand as we identify what it's needed for so that we can just copy it into the final
            // buffer all at once.
            let mut payload = vec![];
            // TODO: Should validate earlier that MAX_FRAME_SIZE is <= usize::max
            payload.resize(header_frame.length as usize, 0);
            reader.read_exact(&mut payload).await?;

            match header_frame.typ {
                FrameType::DATA => {
                    if header_frame.stream_id == 0 {
                        return Err(ProtocolErrorV2 {
                            code: ErrorCode::PROTOCOL_ERROR,
                            message: "DATA frame received on the connection control stream.",
                            local: true
                        }.into());
                    } 

                    let data_flags = DataFrameFlags::parse_complete(&[header_frame.flags])?;
                    let data_frame = DataFramePayload::parse_complete(&payload, &data_flags)?;
                    frame_utils::check_padding(&data_frame.padding)?;

                    /*
                    If a DATA frame is received
                    whose stream is not in "open" or "half-closed (local)" state, the
                    recipient MUST respond with a stream error (Section 5.4.2) of type
                    STREAM_CLOSED.
                    */

                    // Verify stream exists (not still applies to flow control)
                    // Check if remotely closed. Even if closed, we still need it to count towards flow control (so we may want to tell the remote endpoint that more data is available)


                    let mut connection_state = self.state.lock().await;
                    if connection_state.local_connection_window < (header_frame.length as WindowSize) {
                        // TODO: Should we still 
                        return Err(ProtocolErrorV2 {
                            code: ErrorCode::FLOW_CONTROL_ERROR,
                            message: "Exceeded connection level window",
                            local: true
                        }.into());
                    }

                    connection_state.local_connection_window += header_frame.length as WindowSize;

                    // if payload.len() > 

                    // Push data into receiving buffer / update 


                    let stream = match connection_state.streams.get(&header_frame.stream_id) {
                        Some(s) => s,
                        None => {
                            // Send a STREAM_CLOSED 
                            continue;
                        } 
                    };

                    let mut stream_state = stream.state.lock().await;

                    stream_state.received_total_bytes += data_frame.data.len();
                    if data_flags.end_stream {
                        if let Some(expected_size) = stream_state.received_expected_bytes.clone() {
                            if expected_size != stream_state.received_total_bytes {
                                // Error. Mismatch between content-length and data framing.
                            }
                        }
                    }

                    if stream_state.received_end_of_stream {
                        // Send a STREAM_CLOSED error (should be sent even if the stream is closed??)
                    }

                    if stream_state.local_window < (header_frame.length as WindowSize) {
                        // Send a RST_STREAM
                    }
                    stream_state.local_window += header_frame.length as WindowSize;


                    stream_state.received_end_of_stream = data_flags.end_stream;
                    stream_state.received_buffer.extend_from_slice(&data_frame.data);

                    let _ = stream.read_available_notifier.try_send(());
                }
                FrameType::HEADERS => {
                    let headers_flags = HeadersFrameFlags::parse_complete(&[header_frame.flags])?;
                    let headers_frame = HeadersFramePayload::parse_complete(
                        &payload, &headers_flags)?;
                    frame_utils::check_padding(&headers_frame.padding)?;
 
                    // TODO: Check early which stream id is used?

                    let received_headers = ReceivedHeaders {
                        data: headers_frame.header_block_fragment,
                        stream_id: header_frame.stream_id,
                        typ: ReceivedHeadersType::RegularHeaders {
                            end_stream: headers_flags.end_stream,
                            priority: headers_frame.priority
                        }
                    };

                    if headers_flags.end_headers {
                        self.receive_headers(received_headers).await?;
                    } else {
                        pending_header = Some(received_headers);
                    }
                }
                FrameType::PRIORITY => {

                }
                FrameType::RST_STREAM => {
                    if header_frame.stream_id == 0 {
                        return Err(ProtocolErrorV2 {
                            code: ErrorCode::PROTOCOL_ERROR,
                            message: "Received RST_STREAM frame on connection control stream",
                            local: true
                        }.into());
                    }

                    if (header_frame.length as usize) != RstStreamFramePayload::size_of() {
                        return Err(ProtocolErrorV2 {
                            code: ErrorCode::FRAME_SIZE_ERROR,
                            message: "Received RST_STREAM frame of wrong length",
                            local: true
                        }.into());
                    }

                    {
                        let mut connection_state = self.state.lock().await;
                        if (self.is_local_stream_id(header_frame.stream_id) && header_frame.stream_id > connection_state.last_sent_stream_id) ||
                            (self.is_remote_stream_id(header_frame.stream_id) && header_frame.stream_id > connection_state.last_received_stream_id) {
                            return Err(ProtocolErrorV2 {
                                code: ErrorCode::PROTOCOL_ERROR,
                                message: "Received RST_STREAM for idle stream",
                                local: true
                            }.into());
                        }
                    }

                    let rst_stream_frame = RstStreamFramePayload::parse_complete(&payload)?;

                    self.recv_reset_stream(header_frame.stream_id, ProtocolErrorV2 {
                        code: rst_stream_frame.error_code,
                        message: "",
                        local: true
                    }).await?;
                }
                FrameType::SETTINGS => {
                    if header_frame.stream_id != 0 {
                        return Err(ProtocolErrorV2 {
                            code: ErrorCode::PROTOCOL_ERROR,
                            message: "Received SETTINGS frame on non-connection control stream",
                            local: true
                        }.into());
                    }

                    if header_frame.length % 6 != 0 {
                        return Err(ProtocolErrorV2 {
                            code: ErrorCode::FRAME_SIZE_ERROR,
                            message: "Received SETTINGS frame of wrong length",
                            local: true
                        }.into());
                    }

                    let settings_flags = SettingsFrameFlags::parse_complete(&[header_frame.flags])?;
                    let settings_frame = SettingsFramePayload::parse_complete(&payload)?;

                    let mut connection_state = self.state.lock().await;

                    if settings_flags.ack {
                        if header_frame.length != 0 {
                            return Err(ProtocolErrorV2 {
                                code: ErrorCode::FRAME_SIZE_ERROR,
                                message: "Received SETTINGS ACK with non-zero length",
                                local: true
                            }.into());
                        }

                        if connection_state.local_settings_sent_time.is_none() {
                            return Err(ProtocolErrorV2 {
                                code: ErrorCode::PROTOCOL_ERROR,
                                message: "Received SETTINGS ACK while no settings where pending ACK",
                                local: true
                            }.into());
                        }

                        // TODO: Apply any other state changes that are needed.
                        connection_state.local_settings = connection_state.local_pending_settings.clone();
                        connection_state.local_settings_sent_time = None;
                    } else {

                        let mut header_table_size = None;

                        // Apply the settings.
                        for param in settings_frame.parameters {
                            let old_value = connection_state.remote_settings.set(param.id, param.value)?;

                            if param.id == SettingId::HEADER_TABLE_SIZE {
                                // NOTE: This will be applied on the writer thread as it needs to ACK'ed in lock step
                                // with any usages of the header encoder.
                                header_table_size = Some(param.value);
                            } else if param.id == SettingId::INITIAL_WINDOW_SIZE {
                                // NOTE: Changes to this parameter DO NOT update the 

                                let window_diff = (param.value - old_value.unwrap_or(0)) as WindowSize;

                                for (stream_id, stream) in &connection_state.streams {
                                    let mut stream_state = stream.state.lock().await;

                                    if stream_state.sending_at_end && stream_state.sending_buffer.is_empty() {
                                        continue;
                                    }

                                    stream_state.remote_window = stream_state.remote_window.checked_add(
                                        window_diff).ok_or_else(|| Error::from(ProtocolErrorV2 {
                                            code: ErrorCode::FLOW_CONTROL_ERROR,
                                            message: "Change in INITIAL_WINDOW_SIZE cause a window to overflow",
                                            local: true
                                        }))?;

                                    // The window size change may have make it possible for more data to be send.
                                    stream.write_available_notifier.try_send(());
                                }

                                // Force a re-check of whether or not more data can be sent.
                                self.connection_event_channel.0.send(ConnectionEvent::StreamWrite { stream_id: 0 }).await;
                            }
                        }

                        self.connection_event_channel.0.send(ConnectionEvent::AcknowledgeSettings { header_table_size }).await;
                    }
                }
                FrameType::PUSH_PROMISE => {

                }
                FrameType::PING => {
                    if header_frame.stream_id != 0 {
                        return Err(ProtocolErrorV2 {
                            code: ErrorCode::PROTOCOL_ERROR,
                            message: "Received PING message on non-connection control stream",
                            local: true
                        }.into());
                    }

                    if (header_frame.length as usize) != PingFramePayload::size_of() {
                        return Err(ProtocolErrorV2 {
                            code: ErrorCode::FRAME_SIZE_ERROR,
                            message: "Received PING message of wrong length",
                            local: true
                        }.into());
                    }

                    let ping_flags = PingFrameFlags::parse_complete(&[header_frame.flags])?;
                    let ping_frame = PingFramePayload::parse_complete(&payload)?;

                    if ping_flags.ack {

                    } else {
                        // TODO: Block if too many pings in a short period of time.
                        self.connection_event_channel.0.send(ConnectionEvent::Ping { ping_frame }).await
                            .map_err(|_| err_msg("Writer thread closed"))?;
                    }

                }
                FrameType::GOAWAY => {
                    if header_frame.stream_id != 0 {
                        return Err(ProtocolErrorV2 {
                            code: ErrorCode::PROTOCOL_ERROR,
                            message: "Received GOAWAY frame on non-connection control stream",
                            local: true
                        }.into());
                    }

                    // TODO: When a server gracefully shuts down, follow the guidance in Section 6.8

                    /* 
                    TODO: Is this a mandatory part of every implementation:
                    After sending a GOAWAY frame, the sender can discard frames for
                    streams initiated by the receiver with identifiers higher than the
                    identified last stream. 
                    */

                    let goaway_frame = GoawayFramePayload::parse_complete(&payload)?;

                    let mut connection_state = self.state.lock().await;

                    // TODO: Verify that once this is set, we won't generate any new streams.
                    connection_state.error = Some(ProtocolErrorV2 {
                        code: goaway_frame.error_code,
                        message: "Remote GOAWAY received", // TODO: Read the opaque message from the remote packet.
                        local: true
                    });

                    // All streams will ids >= last_stream_id we should report as retryable.
                    // For all other streams:
                    //   If there is a forceful failure, mark them as fully closed.

                    // If we have no error, 


                    if goaway_frame.error_code == ErrorCode::NO_ERROR {
                        // Graceful shutdown.

                        for (stream_id, stream) in &connection_state.streams {
                            // TODO: Only applies to locally initialized streams?
                            if self.is_local_stream_id(*stream_id) && *stream_id > goaway_frame.last_stream_id {
                                // Reset the stream with a 'retryable' error.
                                // Main challenge is deadling with locks.
                            }
                        }

                    } else {

                        // Need to reset all the streams!

                        // Need to return an error but shouldn't ask the writer thread to repeat it.

                        return Ok(());
                    }
                    // 
                    // Send a notification to the other side that we need to GOAWAY

                }
                FrameType::WINDOW_UPDATE => {
                    if (header_frame.length as usize) != WindowUpdateFramePayload::size_of() {
                        // Connection error: FRAME_SIZE_ERROR
                        return Err(ProtocolErrorV2 {
                            code: ErrorCode::FRAME_SIZE_ERROR,
                            message: "Received WINDOW_UPDATE message of wrong length",
                            local: true
                        }.into());
                    }

                    // TODO: Should we block these if received on an idle frame.

                    let window_update_frame = WindowUpdateFramePayload::parse_complete(&payload)?;
                    if window_update_frame.window_size_increment == 0 {
                        if header_frame.stream_id == 0 {
                            return Err(ProtocolErrorV2 {
                                code: ErrorCode::PROTOCOL_ERROR,
                                message: "Received WINDOW_UPDATE with invalid 0 increment",
                                local: true
                            }.into());
                        }

                        // TODO: Send this even if the stream is unknown?
                        self.send_reset_stream(header_frame.stream_id, ProtocolErrorV2 {
                            code: ErrorCode::PROTOCOL_ERROR,
                            message: "Received WINDOW_UPDATE with invalid 0 increment",
                            local: true
                        }).await?;
                        continue;
                    }

                    let mut connection_state = self.state.lock().await;
                    if header_frame.stream_id == 0 {
                        connection_state.remote_connection_window = connection_state.remote_connection_window.checked_add(window_update_frame.window_size_increment as WindowSize).ok_or_else(|| ProtocolErrorV2 {
                            code: ErrorCode::PROTOCOL_ERROR,
                            message: "Overflow in connection flow control window size",
                            local: true
                        })?;
                    } else if let Some(stream) = connection_state.streams.get(&header_frame.stream_id) {
                        let mut stream_state = stream.state.lock().await;
                        
                        // TODO: Make this just a stream error? 
                        stream_state.remote_window = stream_state.remote_window.checked_add(window_update_frame.window_size_increment as WindowSize).ok_or_else(|| ProtocolErrorV2 {
                            code: ErrorCode::PROTOCOL_ERROR,
                            message: "Overflow in stream flow control window size",
                            local: true
                        })?;
                    }
                }
                FrameType::CONTINUATION => {
                    let mut received_headers = pending_header.take().unwrap();
                    
                    let continuation_flags = ContinuationFrameFlags::parse_complete(&[header_frame.flags])?;

                    // NOTE: The entire payload is a header chunk.
                    // TODO: Enforce a max size to the combined header data.
                    received_headers.data.extend_from_slice(&payload);

                    if continuation_flags.end_headers {
                        // Process it now.
                    } else {
                        pending_header = Some(received_headers);
                    }
                }
                FrameType::Unknown(_) => {
                    // According to RFC 7540 Section 4.1, unknown frame types should be discarded.
                }
            }
        }
    }

    async fn receive_headers(self: &Arc<Self>, received_headers: ReceivedHeaders) -> Result<()> {
        // TODO: Check all this logic against the RFC. Right now it's mostly implemented based
        // on common sense.

        // TODO: Make sure that the stream id is non-zero

        let mut connection_state = self.state.lock().await;

        // First deserialize all the headers so that they definately get applied to the production state.
        let headers = connection_state.remote_header_decoder.parse_all(&received_headers.data)?;

        match received_headers.typ {
            ReceivedHeadersType::RegularHeaders { end_stream, priority } => {
                if self.is_server {
                    if !self.is_remote_stream_id(received_headers.stream_id) ||
                       received_headers.stream_id < connection_state.last_received_stream_id ||
                       connection_state.streams.contains_key(&received_headers.stream_id) {
                        return Err(ProtocolErrorV2 {
                            code: ErrorCode::PROTOCOL_ERROR,
                            message: "Headers block received on non-new remotely initialized stream",
                            local: true
                        }.into());
                    }

                    if connection_state.error.is_some() {
                        // When shutting down, don't accept any new streams.
                        // TODO: Trigger a reset stream, although I guess that isn't needed.
                        return Ok(());
                    }

                    // TODO: Only do this if the stream is successfully started?
                    connection_state.last_received_stream_id = received_headers.stream_id;

                    // Make new a new stream

                    let (mut stream, mut incoming_body, outgoing_body) = self.new_stream(
                        &connection_state,  received_headers.stream_id);

                    // TODO: What should we do if the 'Connection' header is present in an HTTP 2 request.

                    let head = process_request_head(headers)?;

                    // TODO: I will also need to verify things like whether or not a Transfer-Encoding is given (which is
                    // limited when )
                    if let Some(len) = crate::header_syntax::parse_content_length(&head.headers)? {
                        let mut stream_state = stream.state.lock().await;
                        stream_state.received_expected_bytes = Some(len);

                        incoming_body.expected_length = Some(len);
                    }
                    // TODO: Verify that Transfer-Encoding header isn't used (not allowed when Content-Length is present).

                    let request = Request {
                        // TODO: Convert these to stream errors if possible?
                        head,
                        // TODO: Need to create a new stream to generate this
                        body: Box::new(incoming_body)
                    };

                    stream.outgoing_response_handler = Some(outgoing_body);

                    stream.processing_tasks.push(task::spawn(self.clone().request_handler_driver(
                        received_headers.stream_id, request)));

                    connection_state.streams.insert(received_headers.stream_id, stream);

                    
                    // I guess we can start the sending task here (but I'd ideally prefer not to do that).
                    // If the body is empty, we should reflect that in the state.

                    // Need to formulate the Request object, then create the remote stream object and get going on getting the response.

                    // Start the new request
                } else {
                    if !self.is_local_stream_id(received_headers.stream_id) ||
                       received_headers.stream_id < connection_state.last_sent_stream_id {
                        // Error
                    }

                    let stream = match connection_state.streams.get_mut(&received_headers.stream_id) {
                        Some(s) => s,
                        None => {
                            // Most likely we closed the stream, so we can just ignore the headers.
                            return Ok(());
                        } 
                    };
                    // NOTE: Because the stream is still present in the 'streams' map, we know right here
                    // that it isn't closed yet.

                    // TODO: Maybe update priority

                    let (response_handler, incoming_body) = match stream.incoming_response_handler.take() {
                        Some(v) => v,
                        None => {
                            return Err(ProtocolErrorV2 {
                                code: ErrorCode::PROTOCOL_ERROR,
                                message: "Received headers while not waiting for a response",
                                local: true
                            }.into());
                        }
                    };

                    let response = Response {
                        head: process_response_head(headers)?,
                        body: Box::new(incoming_body)
                    };

                    response_handler.handle_response(Ok(response)).await;
                }
            }
            ReceivedHeadersType::PushPromise { promised_stream_id } => {
                return Err(err_msg("Push promise not yet implemented"));
            }
        }

        Ok(())
    }

    async fn request_handler_driver(self: Arc<Self>, stream_id: StreamId, request: Request) {
        let request_handler = self.request_handler.as_ref().unwrap();

        let response = request_handler.handle_request(request).await;

        let _ = self.connection_event_channel.0.send(ConnectionEvent::SendResponse {
            stream_id: stream_id,
            response
        }).await;

        // TODO: Consider starting the processing task for reading the outgoing body here.
        // This will require us to validate the stream is still open, but this may help with latency.
    }

    /// Needs to listen for a bunch of stuff:
    /// - 
    async fn run_write_thread(&self, mut writer: Box<dyn Writeable>, upgrade_payload: Option<Box<dyn Readable>>,) -> Result<()> {
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
        
        {
            if !self.is_server {
                writer.write_all(CONNECTION_PREFACE).await?;
            }

            let mut state = self.state.lock().await;

            let mut payload = vec![];
            state.local_pending_settings.serialize_payload(&state.local_settings, &mut payload);

            let mut frame = vec![];
            FrameHeader { length: payload.len() as u32, typ: FrameType::SETTINGS, flags: 0, reserved: 0, stream_id: 0 }
                .serialize(&mut frame).unwrap();
            frame.extend(payload);
            writer.write_all(&frame).await?;

            state.local_settings_sent_time = Some(Utc::now());
        }

        // TODO: Block right here if we haven't yet received the remote settings.

        // Used to encode locally created headers to be sent to the other endpoint.
        // NOTE: This is shared across all streams on the connection.
        let mut local_header_encoder = {
            let state = self.state.lock().await;
            hpack::Encoder::new(
                state.remote_settings[SettingId::HEADER_TABLE_SIZE] as usize)
        };

        loop {
            // TODO: If we are gracefully shutting down, stop waiting for events once all pending
            // streams have been closed.

            let event = self.connection_event_channel.1.recv().await?;
            println!("Got ConnectionEvent!");
            match event {
                ConnectionEvent::SendRequest { request, response_handler } => {
                    // TODO: Only allow if we are a client.
                    
                    println!("Sending request...");

                    // TODO: If anything in here fails, we should report it to the requester rather than
                    // killing the whole thread.

                    let mut connection_state = self.state.lock().await;

                    // TODO: Write the rest of the headers (all names should be converted to ascii lowercase)
                    // (aside get a reference from the RFC)

                    let stream_id = {
                        if connection_state.last_sent_stream_id == 0 {
                            if self.is_server { 2 } else { 1 }
                        } else {
                            connection_state.last_sent_stream_id + 2
                        }
                    };
                    connection_state.last_sent_stream_id = stream_id;

                    let (mut stream, incoming_body, outgoing_body) = self.new_stream(
                        &connection_state, stream_id);

                    // Apply client request specific details to the stream's state. 
                    let local_end = {
                        stream.incoming_response_handler = Some((response_handler, incoming_body));

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
                            stream.processing_tasks.push(task::spawn(outgoing_body.run(request.body)));
                            false
                        }
                    };

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
                    let mut connection_state = self.state.lock().await;

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
                            stream.processing_tasks.push(task::spawn(outgoing_body.run(response.body)));
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
                ConnectionEvent::Closing => {
                    // TODO: this should receive an event from the other thread.
                    return Ok(());
                }
                ConnectionEvent::Goaway { last_stream_id, error } => {
                    writer.write_all(&frame_utils::new_goaway_frame(last_stream_id, error)).await?;
                }
                ConnectionEvent::AcknowledgeSettings { header_table_size } => {
                    if let Some(value) = header_table_size {
                        local_header_encoder.set_protocol_max_size(value as usize);
                    }

                    writer.write_all(&frame_utils::new_settings_ack_frame()).await?;
                }
                ConnectionEvent::ResetStream { stream_id, error } => {
                    writer.write_all(&frame_utils::new_rst_stream_frame(stream_id, error)).await?;
                }
                ConnectionEvent::Ping { ping_frame } => {
                    writer.write_all(&frame_utils::new_ping_frame(ping_frame.opaque_data, true)).await?;
                }
                ConnectionEvent::StreamRead { stream_id, count } => {
                    // NOTE: The stream level flow control is already updated in the IncomingStreamBody.
                    self.state.lock().await.local_connection_window += count as WindowSize;

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
                ConnectionEvent::StreamWrite { stream_id } => {
                    let connection_state = self.state.lock().await;
                
                    let max_frame_size = connection_state.remote_settings[SettingId::MAX_FRAME_SIZE];
    
                    let mut next_frame = None;
    
                    for (stream_id, stream) in &connection_state.streams {
                        if connection_state.remote_connection_window <= 0 {
                            break;
                        }
    
                        // TODO: This will probably deadlock with other threads which lock the stream first.
                        let mut stream_state = stream.state.lock().await;
    
                        let min_window = std::cmp::min(
                            connection_state.remote_connection_window,
                            stream_state.remote_window) as usize;
    
                        let n_raw = std::cmp::min(min_window, stream_state.received_buffer.len());
                        let n = std::cmp::min(n_raw, max_frame_size as usize);
                        
                        if n == 0 {
                            continue;
                        }
    
                        let remaining = stream_state.received_buffer.split_off(n);
                        next_frame = Some((*stream_id, stream_state.received_buffer.clone()));
                        stream_state.received_buffer = remaining;

                        let _ = stream.write_available_notifier.try_send(());
                        break;
                    }
    
                    // Drop all locks.
                    drop(connection_state);
    
                    // Write out the next frame.
                    // TODO: To avoid so much copying, consider never sending until we have one full 'chunk' of data.
                    if let Some((stream_id, frame_data)) = next_frame {
                        let frame = frame_utils::new_data_frame(stream_id, frame_data);
                        writer.write_all(&frame).await?;
                    }
                }
            }
        }
    }

    fn new_stream(
        &self, connection_state: &ConnectionState, stream_id: StreamId
    ) -> (Stream, IncomingStreamBody, OutgoingStreamBody) {
        // NOTE: These channels only act as a boolean flag of whether or not something has changed so we should
        // only need to ever have at most 1 message in each of them.
        let (read_available_notifier, read_available_receiver) = channel::bounded(1);
        let (write_available_notifier, write_available_receiver) = channel::bounded(1);
        
        let stream = Stream {
            read_available_notifier,
            write_available_notifier,
            incoming_response_handler: None,
            outgoing_response_handler: None,
            processing_tasks: vec![],
            state: Arc::new(Mutex::new(StreamState {
                // weight: 16, // Default weight
                // dependency: 0,

                error: None,
                
                local_window: connection_state.local_settings[SettingId::INITIAL_WINDOW_SIZE] as WindowSize,
                remote_window: connection_state.remote_settings[SettingId::INITIAL_WINDOW_SIZE] as WindowSize,

                received_buffer: vec![],
                received_end_of_stream: false,
                received_expected_bytes: None,
                received_total_bytes: 0,

                sending_buffer: vec![],
                sending_at_end: false
            }))
        };

        let incoming_body = IncomingStreamBody {
            stream_id,
            stream_state: stream.state.clone(),
            connection_event_sender: self.connection_event_channel.0.clone(),
            read_available_receiver,
            expected_length: None
        };

        let outgoing_body = OutgoingStreamBody {
            stream_id,     
            stream_state: stream.state.clone(),
            connection_event_sender: self.connection_event_channel.0.clone(),
            write_available_receiver
        };

        (stream, incoming_body, outgoing_body)
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
