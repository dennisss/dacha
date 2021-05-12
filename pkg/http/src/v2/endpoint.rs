use std::sync::Arc;
use std::collections::HashMap;

use common::{chrono::Duration, errors::*};
use common::io::{ReadWriteable, Writeable, Readable};
use common::async_std::channel;
use common::async_std::sync::Mutex;
use common::async_std::task;
use common::futures::select;
use common::chrono::prelude::*;

use crate::v2::settings::*;
use crate::hpack;
use crate::request::Request;
use crate::response::Response;
use crate::server::RequestHandler;
use crate::proto::v2::*;
use crate::body::Body;

const CONNECTION_PREFACE: &[u8] = b"PRI * HTTP/2.0\r\n\r\nSM\r\n\r\n";

type StreamId = u32;

/// Type used to represent the size of the flow control window.
///
/// NOTE: The window may go negative.
type WindowSize = i32;

const FLOW_CONTROL_MAX_SIZE: WindowSize = ((1u32 << 1) - 1) as i32;

const MAX_STREAM_ID: StreamId = (1 << 31) - 1;

/// Amount of time after which we'll close the connection if we don't receive an acknowledment to our
/// 
const SETTINGS_ACK_TIMEOUT: Duration = Duration::seconds(10);

// TODO: Should also use PING to countinuously verify that the server is still alive.
//
//  Received a GOAWAY with error code ENHANCE_YOUR_CALM and debug data equal to "too_many_pings"
// https://github.com/grpc/grpc/blob/fd3bd70939fb4239639fbd26143ec416366e4157/doc/keepalive.md
//
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

#[derive(Debug, Clone, Fail)]
struct ProtocolError {
    code: ErrorCode,
    message: &'static str
}


type ProtocolResult<T> = std::result::Result<T, ProtocolError>;

struct Stream {
    /// Internal state variables used by multiple threads.
    state: Arc<Mutex<StreamState>>,

    /// Used to let the body object know that data is available to be read.
    read_available_notifier: channel::Sender<()>,

    /// Used to let the local thread that is processing this stream know that
    /// more data can be written to the stream.
    write_available_notifier: channel::Sender<()>,
}

// TODO: 

/// Variable state associated with the stream.
/// NOTE: 
/// TODO: Split into reader and writer states 
struct StreamState {
    // state: StreamState,

    /// If true, then this stream is in a non-closed state.
    ///
    /// This will be set to false once the RST_STREAM packet is sent.  
    active: bool,

    weight: u8,

    dependency: StreamId,

    /// Task used to read local data to send to the remote endpoint.
    /// We retain a handle to this so that we can cancel it if we need to abrutly close the stream.
    ///
    /// TODO: Ensure that this is ALWAYS cancelled when the stream or connection is garbage collected.
    sending_task: task::JoinHandle<()>,

    /// If there was a stream or connection level error, it will be stored here, otherwise this will be
    /// Ok(). Additionally if we saw an error while 
    error: Option<ProtocolError>,

    /// Number of bytes of data the local endpoint is willing to accept from the remote endpoint for
    /// this stream. 
    local_window: WindowSize,

    /// Data which has been received from the remote endpoint as part of DATA frames but hasn't
    /// been read by the stream handler yet.
    ///
    /// TODO: Make this a cyclic buffer or a list of chunked buffers. (the challenge with a cyclic
    /// buffer is that we should block accidentally overriding data)
    received_buffer: Vec<u8>,

    /// If true, aside from what is in 'received_buffer', we have received all data on this stream from
    /// the remote endpoint.
    received_end_of_stream: bool,

    /// Data waiting to be sent to the remote endpoint.
    /// TODO: Need to be sinegat restrictive about how big this can get (can't use remote_window as the
    /// max for this as that may be an insanely large number)
    sending_buffer: Vec<u8>,

    /// Whether or not 
    sending_at_end: bool,

    /// Number of bytes the remote endpoint is willing to accept from the local endpoint for
    /// this stream.
    remote_window: WindowSize,
}

/*
    Eventually we want to have a HTTP2 specific wrapper around a Request/Response to support
    changing settings, assessing stream/connection ids, or using the push functionality.

*/


pub trait ResponseHandler {
    fn handle_response(&self, response: Result<Response>);
}


/// Event emitted when a local task has consumed some data from a stream.
struct ReadEvent {
    stream_id: StreamId,
    count: usize
}

// TODO: Should we support allowing the connection itself to stay half open.

/// NOTE: One instance of this should 
pub struct Connection {
    is_server: bool,

    state: Arc<Mutex<ConnectionState>>,

    // TODO: We may want to keep around a timer for the last time we closed a stream so that if we 

    /// Channel used to listen for requests initiated locally that should be sent
    /// to the remote endpoint.
    ///
    /// NOTE: This will only be used in HTTP clients. 
    request_channel: (channel::Sender<(Request, Box<dyn ResponseHandler>)>,
                      channel::Receiver<(Request, Box<dyn ResponseHandler>)>),

    /// Handler for producing responses to incoming requests.
    ///
    /// NOTE: This will only be used in HTTP servers.
    request_handler: Box<dyn RequestHandler>,

    /// When a local stream handler has read some data out of the internal 'received_buffer', it will
    /// signal this by sending a message on this channel. In response the connection will do things
    /// will notify the remote endpoint that the size of the window has changed.
    read_complete_channel: (channel::Sender<ReadEvent>, channel::Receiver<ReadEvent>),
    
    /// When a local stream body generator has produced data that can be sent to the remote endpoint,
    /// it will send a notification through this channel.
    ///
    /// In response, the connection will attempt to send more data to the remote endpoint (if allowable
    /// in flow control).
    write_complete_channel: (channel::Sender<StreamId>, channel::Receiver<StreamId>),

    /// Used to enqueue frames that need to be send to the remote endpoint.
    /// Sending these must be prioritized over sending other frames as these have already been applied to
    /// the state of the connection/streams.
    ///
    /// TODO: If we need to send a GOAWAY, prioritize sending that other messages like RST_STREAM or PING?
    internal_frame_queue: (channel::Sender<Vec<u8>>, channel::Receiver<Vec<u8>>),

    // Stream ids can't be re-used.
    // Also, stream ids can't be 

    // How to implement a request:
    // - Allowed to acquire a lock to the connection state and underlying writer,
    //    - It should block if the flow control window is exceeded.
    // ^ But wait, we don't support a push model, only a pull model?

    // So do we want to poll all the distinct streams?
    // - Probably not. would rather create one task per stream.
    // - It will loop trying to read as much as we can until we exceed the remote flow control limit?
    // - We'll have a separate priority queue of which data is available to be sent.

    
}

/// Volatile data associated with the connection.
struct ConnectionState {
    /// Whether or not run() was ever called on this connection.
    running: bool,

    error: Option<ProtocolError>,

    // TODO: Shard this into the reader and writer states.

    /// Used to encode locally created headers to be sent to the other endpoint.
    /// NOTE: This is shared across all streams on the connection.
    local_header_encoder: hpack::Encoder,

    /// Used to decode remotely created headers received on the connection.
    /// NOTE: This is shared across all streams on the connection.
    remote_header_decoder: hpack::Decoder,

    /// Settings currently in use by this endpoint.
    local_settings: SettingsContainer,

    /// Time at which the 'local_pending_settings' were sent to the remote server.
    /// A value of None means that no settings changes are pending.
    local_settings_sent_time: Option<DateTime<Utc>>,

    /// Next value of 'local_settings' which is pending acknowledgement from the other endpoint.
    local_pending_settings: SettingsContainer,
    
    /// Number of data bytes we are willing to accept on the whole connection.
    local_connection_window: WindowSize,

    remote_settings: SettingsContainer,
    remote_connection_window: WindowSize,

    last_received_stream_id: StreamId,
    last_sent_stream_id: StreamId,

    /// All currently active locally and remotely initialized streams.
    streams: HashMap<StreamId, Stream>,
}

impl Connection {

    // TODO: Need to support initializing with settings already negiotated via HTTP

    // TODO: Verify that run is never called more than once on the same Connection instance.
    pub async fn run(&mut self, reader: Box<dyn Readable>, writer: Box<dyn Writeable>) -> Result<()> {
        let mut state = self.state.lock().await;

        if state.running {
            return Err(err_msg("run() can only be called once per connection"));
        }
        state.running = true;

        let mut reader_task = task::spawn(self.run_read_thread(reader));
        let mut writer_task = task::spawn(self.run_write_thread(writer));

        // select! {
        //     read_res = 
        // }
        
        // TODO: Ensure that the first set of settings are acknowledged.


        // We should have a reader task whose only responsibility is to read from the 

        // 


        // pipe.read_exact(&mut )

        // select! {
        //     value = pipe.re

        // }




        // common::async_std::task::spawn(future)

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

        /*
            Events to look out for:
            - New packets received from remote endpoint
            - Local 'requests'
                - A local request will contain the headers and other info needed to init the stream
                - Will respond back with a stream id which can be used to read or write stuff.
                - The main thread will wait for an mpsc queue

            - A response will be a buffered quueue (for writing a request body we could just hold an Arc<Mutex> to the connection and wait for it to become free to be able to send more data?)
                - Issue is that we can't hold for too long.


            - NOTE: The connection will buffer any received data which hasn't yet been read 

        */

        Ok(())
    }

    // 
    
    // TODO: According to RFC 7540 Section 4.1, undefined flags should be left as zeros.

    /*
        How to handle a conection error:
        - 
    */

    /// Runs the thread that is the exlusive reader of incoming data from the raw connection.
    ///
    /// Internal Error handling:
    /// - If a connection error occurs, this function will return immediately with a non-ok result.
    ///   The caller should communicate this to the 
    ///
    /// External Error Handling:
    /// - The caller should cancel this future when it wants to 
    async fn run_read_thread(&self, mut reader: Box<dyn Readable>) -> Result<()> {
        let mut preface = [0u8; CONNECTION_PREFACE.len()];
        reader.read_exact(&mut preface).await?;


        let mut frame_header_buf = [0u8; FrameHeader::size_of()];

        // If the read thread fails, we should tell the write thread to complain about an error.
        // Likewise we need to be able to send other types of events to the write thread.

        // TODO: Receiving any packet on a stream with a smaller number than any stream id ever seen
        // should casue an error.

        // let mut pending_header_

        //
        loop {
            reader.read_exact(&mut frame_header_buf).await?;

            let header_frame = FrameHeader::parse_complete(&frame_header_buf)?;

            let max_frame_size = {
                let state = self.state.lock().await;
                state.local_settings[SettingId::MAX_FRAME_SIZE]
            };

            // Error handling based on RFC 7540: Section 4.2
            if header_frame.length > max_frame_size {
                let can_alter_state =
                    header_frame.typ == FrameType::SETTINGS ||
                    header_frame.typ == FrameType::HEADERS ||
                    header_frame.typ == FrameType::PUSH_PROMISE ||
                    header_frame.typ == FrameType::CONTINUATION ||
                    header_frame.stream_id == 0;
                
                if can_alter_state {
                    // REturn a CONNECTION Error
                    // FRAME_SIZE_ERROR
                } else {
                    // Stream: FRAME_SIZE_ERROR
                }

                // Read until completion.

                // Skip over this frame's payload by just reading into a waste buffer until we reach
                // the end of the packet.
                let mut num_remaining = header_frame.length as usize;
                while num_remaining > 0 {
                    let mut buf = [0u8; 512];
                    let n = reader.read(
                        &mut buf[0..std::cmp::min(num_remaining, buf.len())]).await?;
                    num_remaining -= n;

                    if n == 0 {
                        return Ok(())
                    }
                }

                continue;
            }

            let mut payload = vec![];
            // TODO: Should validate earlier that MAX_FRAME_SIZE is <= usize::max
            payload.resize(header_frame.length as usize, 0);
            reader.read_exact(&mut payload).await?;

            match header_frame.typ {
                FrameType::DATA => {
                    if header_frame.stream_id == 0 {
                        return Err(ProtocolError {
                            code: ErrorCode::PROTOCOL_ERROR,
                            message: "DATA frame received on the connection stream."
                        }.into());
                    } 

                    let data_flags = DataFrameFlags::parse_complete(&[header_frame.flags])?;
                    let data_frame = DataFramePayload::parse_complete(&payload, &flags)?;

                    for byte in data_frame.padding {
                        if byte != 0 {
                            return Err(ProtocolError {
                                code: ErrorCode::PROTOCOL_ERROR,
                                message: "Received non-zero padding in DATA frame"
                            });
                        }
                    }

                    /*
                    If a DATA frame is received
                    whose stream is not in "open" or "half-closed (local)" state, the
                    recipient MUST respond with a stream error (Section 5.4.2) of type
                    STREAM_CLOSED.
                    */

                    // Verify stream exists (not still applies to flow control)
                    // Check if remotely closed. Even if closed, we still need it to count towards flow control (so we may want to tell the remote endpoint that more data is available)


                    let connection_state = self.state.lock().await;
                    if connection_state.local_connection_window < (header_frame.length as WindowSize) {
                        // 
                    }

                    // if payload.len() > 

                    // Check flow control

                    // Push data into receiving buffer / update 
                }
                FrameType::HEADERS => {
                    if !self.is_server {
                        // Error? TODO: Find a reference in the RFC
                    }

                    // Need to wait for more frames (exclusively CONTINUATION frames)
                    // we need to 
                }
                FrameType::PRIORITY => {

                }
                FrameType::RST_STREAM => {
                    // Mark stream as failed.
                    // Send this 
                }
                FrameType::SETTINGS => {
                    // Need to immediately validate and apply the new settings. 
                }
                FrameType::PUSH_PROMISE => {

                }
                FrameType::PING => {
                    if header_frame.stream_id != 0 {
                        // Connection error: PROTOCOL_ERROR
                    }

                    if (header_frame.length as usize) != PingFramePayload::size_of() {
                        // Connection error: FRAME_SIZE_ERROR
                    }

                    let ping_frame = PingFramePayload::parse_complete(&payload)?;

                    // 

                }
                FrameType::GOAWAY => {
                    // 
                }
                FrameType::WINDOW_UPDATE => {
                    // Check that 

                    if (header_frame.length as usize) != WindowUpdateFramePayload::size_of() {
                        // Connection error: FRAME_SIZE_ERROR
                    }


                    // Section 6.9: An increment of 0 is a PROTOCOL_ERROR stream error.
                    // But, if it's on the connection window, then 

                }
                FrameType::CONTINUATION => {

                }
                FrameType::Unknown(_) => {
                    // According to RFC 7540 Section 4.1, unknown frame types should be discarded.
                }
            }

            // let mut payload = vec![];
            // payload.reserve()


            // MAX_FRAME_SIZE

            


        }


    }


    fn new_window_update_frame(stream_id: StreamId, increment: usize) -> Vec<u8> {
        let mut data = vec![];
        FrameHeader {
            typ: FrameType::WINDOW_UPDATE,
            length: WindowUpdateFramePayload::size_of() as u32,
            flags: 0,
            reserved: 0,
            stream_id
        }.serialize(&mut data);

        WindowUpdateFramePayload {
            reserved: 0,
            window_size_increment: increment as u32,
        }.serialize(&mut data).unwrap();

        data
    }

    fn new_data_frame(stream_id: StreamId, data: Vec<u8>) -> Vec<u8> {
        let mut frame = vec![];
        FrameHeader {
            typ: FrameType::DATA,
            flags: DataFrameFlags {
                padded: false,
                end_stream: false,
                reserved1: 0,
                reserved2: 0
            }.to_u8().unwrap(),
            length: data.len() as u32,
            reserved: 0,
            stream_id
        }.serialize(&mut frame).unwrap();

        frame.extend_from_slice(&data);

        frame
    }

    /// Needs to listen for a bunch of stuff:
    /// - 
    async fn run_write_thread(&self, mut writer: Box<dyn Writeable>) -> Result<()> {
        {
            writer.write_all(CONNECTION_PREFACE).await?;

            let state = self.state.lock().await;

            let mut payload = vec![];
            state.local_pending_settings.serialize_payload(state.local_settings, &mut payload);

            let mut frame = vec![];
            FrameHeader { length: payload.len() as u32, typ: FrameType::SETTINGS, flags: 0, reserved: 0, stream_id: 0 }
                .serialize(&mut frame);
            frame.extend(payload);
            writer.write_all(frame).await?;

            state.local_settings_sent_time = Utc::now();
        }

        loop {
            {
                // When we have read received data we'll send an update to the remote endpoint of our progress.
                // TODO: Ideally batch these so that individual reads can't be used to determine internal control
                // flow state. 
                let e = self.read_complete_channel.1.recv().await?;
                writer.write_all(&Self::new_window_update_frame(0, e.count)).await?;
                writer.write_all(&Self::new_window_update_frame(e.stream_id, e.count)).await?;
            }

            // Write event:
            // - Happens on either remote flow control updates or 
            {
                let e = self.write_complete_channel.1.recv().await;

                let connection_state = self.state.lock().await;
                
                let max_frame_size = connection_state.remote_settings[SettingId::MAX_FRAME_SIZE];

                let mut next_frame = None;

                for (stream_id, stream) in &connection_state.streams {
                    if connection_state.remote_connection_window <= 0 {
                        break;
                    }

                    // TODO: This will probably deadlock with other threads which lock the stream first.
                    let stream_state = stream.state.lock().await;

                    let min_window = std::cmp::min(
                        connection_state.remote_connection_window,
                        stream_state.remote_window) as usize;

                    let n_raw = std::cmp::min(min_window, stream_state.received_buffer.len());
                    let n = std::cmp::min(n, max_frame_size as usize);
                    
                    if n == 0 {
                        continue;
                    }

                    let remaining = stream_state.received_buffer.split_off(n);
                    next_frame = Some((*stream_id, stream_state.received_buffer));
                    stream_state.received_buffer = remaining;
                    break;
                }

                // Drop all locks.
                drop(connection_state);

                // Write out the next frame.
                // TODO: To avoid so much copying, consider never sending until we have one full 'chunk' of data.
                if let Some((stream_id, frame_data)) = next_frame {
                    let frame = Self::new_data_frame(stream_id, frame_data);
                    writer.write_all(frame).await?;
                }
            }

            // Internal events:
            // - Ping (needs a response)
            // - Errors


            /*
            select!(
                // TODO: Prioritize this one.
                frame = self.internal_frame_queue.1.recv() => {
                    writer.write(&frame).await?;
                }
                (request, response_handler) = self.request_handler.1.recv() => {
                    // Send the request on a new stream.

                    // Save the response handler for later (when we get the headers)

                }


            );
            */


        }

        // If the read thread wants to ACK something, we need to ack accordinly.

    }

    // async fn start_request

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


enum ConnectionReaderEvent {
    /// We received a ping from the remote endpoint. In response, we should respond with an ACK
    Ping(PingFramePayload),

    /// We
    WindowUpdate(usize),

}


/// Wrapper around a Body that is used to read it and feed it to a stream.
/// This is intended to be run as a separate 
struct OutgoingStreamBodyReader {
    body: Box<dyn Body>,

    stream_id: StreamId,

    connection_state: Arc<Mutex<ConnectionState>>,
    stream_state: Arc<Mutex<StreamState>>,

    write_complete_notifier: channel::Sender<StreamId>,
    write_available_notifier: channel::Receiver<()>
}

impl OutgoingStreamBodyReader {

    pub async fn run(mut self) {
        if let Err(e) = self.run_internal().await {
            // TODO: Re-use guard from run_internal
            let mut stream = self.stream_state.lock().await;
            // stream.
        }
    }

    async fn run_internal(&mut self) -> Result<()> {

        loop {
            // TODO: We don't want to be locking this for a long time. that may prevent 
            let mut stream = self.stream_state.lock().await;
            // TODO: Stop immediately if we are locally closed.
            
            

        }

        // Periodically 

        // Eventually we need to close it.

        Ok(())
    }

}



/// Reader of data received on a HTTP2 stream from a remote endpoint.
///
/// TODO: Sometimes we may want read() to return an error (e.g. if there was a stream error.)
/// TODO: Dropping this object should imply what?
struct IncomingStreamBody {
    stream_id: StreamId,

    connection_state: Arc<Mutex<ConnectionState>>,

    stream_state: Arc<Mutex<StreamState>>,

    /// Used by the body to notify the connection that data has been read.
    /// This means that the connection can let the other side know that more
    /// data can be sent. 
    ///
    /// NOTE: This is created by cloning the 'read_complete_channel' Sender in the 'Connection'.
    read_complete_notifier: channel::Sender<StreamId>,

    /// Used by the body to wait for more data to become available to read from the stream (or for an error to occur).
    read_available_notifier: channel::Receiver<()>,
}

#[async_trait]
impl Readable for IncomingStreamBody {
    async fn read(&mut self, mut buf: &mut [u8]) -> Result<usize> {
        let mut nread = 0;

        // TODO: Error out if this has to loop more than twice.
        while !buf.is_empty() {
            let mut connection_state = self.connection_state.lock().await;
            let mut stream_state = self.stream_state.lock().await;

            if let Some(e) = &connection_state.error {
                return Err(e.into());
            }

            if let Some(e) = &stream_state.error {
                return Err(e.into());
            }
 
            if !stream_state.received_buffer.is_empty() {
                let n = std::cmp::min(buf.len(), stream_state.received_buffer.len());
                (&mut buf[0..n]).copy_from_slice(&stream_state.received_buffer[0..n]);
                buf = &mut buf[n..];

                // TODO: Not verify efficient
                stream_state.received_buffer = stream_state.received_buffer.split_off(n);

                // Allow the remote endpoint to send more data now that some has been read.
                // TODO: Optimize this so that we only need the channel to store a HashSet of stream ids
                // TODO: Also no point in calling this is the stream is remotely closed.
                stream_state.local_window += n;
                {
                    // TODO: Verify that this can't deadlock.
                    connection_state.local_connection_window += n;
                }
                self.read_complete_notifier.send(self.stream_id).await?;

                nread += n;

                // Stop as soon as we read any data
                break;
            } else if stream_state.received_end_of_stream {
                break;
            }

            // Unlock all resources.
            drop(stream);
            drop(connection_state);

            // Wait for a change in the reader buffer.
            self.read_available_notifier.recv().await?;
        }

        Ok(nread)
    }
}
