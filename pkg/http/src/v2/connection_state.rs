use std::sync::Arc;
use std::collections::HashMap;
use std::collections::VecDeque;

use common::errors::Result;
use common::task::ChildTask;
use common::async_std::channel;
use common::async_std::sync::Mutex;

use crate::proto::v2::*;
use crate::v2::settings::*;
use crate::v2::types::*;
use crate::request::Request;
use crate::response::{Response, ResponseHandler};
use crate::v2::stream::Stream;
use crate::v2::stream_state::StreamState;

/// Volatile data associated with the connection.
pub struct ConnectionState {
    /// Whether or not run() was ever called on this Connection.
    /// This is mainly used to ensure that at most one set of reader/writer threads are associated with the connection.
    pub running: bool,
    
    /// Will be taken by the ConnectionWriter once it starts running.
    pub connection_event_receiver: Option<channel::Receiver<ConnectionEvent>>,

    // TODO: Need to have solid understanding of the GOAWAY state where we are gracefully shutting down but there was no error.
    // Likewise if we send a GOAWAY, we shoulnd't create new streams?? (e.g. no more requests or PUSH_PROMISES??)

    /// If present, then the connection is either shutting down or already
    /// closed.
    pub shutting_down: ShuttingDownState,

    /// Used by an client to enqueue requests to be sent to the other endpoint.
    /// If a request is still in this list then it definately hasn't been sent to the other
    /// endpoint yet.
    ///
    /// On a client, the total number of outstanding requests is:
    ///     'pending_requests.len() + local_open_stream_count'
    ///
    /// TODO: If the client side hands up on a request either before it was sent or before we were done receiving the response
    /// we need to ensure that we can cancel the request. 
    pub pending_requests: VecDeque<ConnectionLocalRequest>,

    // TODO: Shard this into the reader and writer states.

    /// Settings currently in use by this endpoint.
    pub local_settings: SettingsContainer,

    /// If we have sent out settings that are still pending acknowledgement
    /// from the remote server, then this will be thread which waits for
    /// a timeout to elapse before we close the connection.
    pub local_settings_ack_waiter: Option<ChildTask>,

    /// Next value of 'local_settings' which is pending acknowledgement from the other endpoint.
    pub local_pending_settings: SettingsContainer,
    
    /// Number of data bytes we are willing to accept on the whole connection.
    pub local_connection_window: WindowSize,

    pub remote_settings: SettingsContainer,

    /// Whether or not we have received some set of settings from the remote endpoint
    /// (e.g. the HTTP2-Settings header or an initial SETTINGS frame)
    /// Else, the remote_settings field will only contain the default settings defined
    /// by the HTTP2 protocol.
    pub remote_settings_known: bool,

    pub remote_connection_window: WindowSize,

    pub last_received_stream_id: StreamId,
    pub last_sent_stream_id: StreamId,

    /// The highest stream id which we will accept from the remote endpoint. By default this will
    /// be MAX_STREAM_ID, but will be decreased during graceful shutdown.
    pub upper_received_stream_id: StreamId,

    /// The highest stream id which we are allowed to send.
    /// Will be decreased whenever we receive a GOAWAY packet.
    pub upper_sent_stream_id: StreamId,

    /// Number of locally initialized 
    pub local_stream_count: usize,

    pub remote_stream_count: usize,

    /// All currently active locally and remotely initialized streams.
    pub streams: HashMap<StreamId, Stream>,
}

/// Event received by the writer thread of the connection from other processing
/// threads. Most of these require that the writer take some action in response
/// to the event.
pub enum ConnectionEvent {
    /// We received a ping from the remote endpoint. In response, we should respond with an ACK.
    ///
    /// Sender: Connection level reader thread
    Ping {
        ping_frame: PingFramePayload
    },

    /// There was an error while processing a stream. We should tell the remote endpoint that we
    /// are closing the stream prematurely.
    ResetStream {
        stream_id: StreamId,
        error: ProtocolErrorV2
    },

    /// A locally initialized GOAWAY was triggered. In response, we should let the other endpoint
    /// know about it.
    // Goaway {
        

    //     /// If present, then this was a local internal error was generated (not the fault of the
    //     /// remote endpoint and this was the reason why we are )
    //     close_with: Option<Result<()>>
    // },
    
    /// The connection is ready to be closed immediately.
    ///
    /// This means that either:
    /// 1. All streams are closed, so error == None and we are done gracefully shutting down.
    /// 2. We received a remote GOAWAY with a non-NO_ERROR code or the reader thread failed, so we
    ///    should close the connection ASAP.


    /// NOTE: If you send this event, you are responsible for setting
    /// ConnectionState::shutting_down and ConnectionState::upper_received_stream_id appropriately. 
    Closing {
        send_goaway: Option<ProtocolErrorV2>,
        close_with: Option<Result<()>>,
    },

    /// We received remote settings which we've applied to the local state and should now be
    /// acknowledged.
    ///
    /// Sender: Connection level reader thread
    AcknowledgeSettings {
        header_table_size: Option<u32>
    },

    /////

    /// A local task hsa consumed some data from a stream. In response, we should update our flow
    /// control and allow the remote endpoint to send us more data.
    StreamRead {
        stream_id: StreamId,
        count: usize
    },

    /// Indicates that no more data will ever be read from the given stream.
    /// In response, we can drop any future data received on this stream.
    StreamReaderClosed {
        stream_id: StreamId,
        stream_state: Arc<Mutex<StreamState>>
    },

    StreamWrite {
        stream_id: StreamId
    },

    /// Sent when the OutgoingStreamBody fails to generate more data to write into the stream.
    /// In response, we should close the stream.
    StreamWriteFailure {
        stream_id: StreamId,
        internal_error: common::errors::Error
    },

    /// We are an HTTP client connection and a locally generated request needs to be sent to the
    /// other endpoint.
    SendRequest,

    // NOTE: Because the DATA frames should ideally follow the header frames, the writer
    // frame will be the one responsible for starting the reading task for this stream once
    // it gets this event. 
    SendResponse {
        stream_id: StreamId,
        response: Response,
    },

    SendPushPromise {
        request: Request,
        response: Response
    },
}

pub enum ShuttingDownState {
    No,

    /// We received a remote GOAWAY
    Remote,

    Graceful {
        /// Task which eventually triggers a transition to the Abrupt shutdown state.
        /// Only used on Server endpoints to ensure that the server shutdown time is bounded.
        timeout_task: Option<ChildTask>
    },

    Abrupt,

    Complete
}

impl ShuttingDownState {
    pub fn is_some(&self) -> bool {
        if let ShuttingDownState::No = self {
            false
        } else {
            true
        }
    }
}

pub struct ConnectionLocalRequest {
    pub request: Request,
    pub response_handler: Box<dyn ResponseHandler>
}
