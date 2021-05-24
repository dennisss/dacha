use std::collections::HashMap;

use common::chrono::prelude::*;

use crate::proto::v2::*;
use crate::v2::settings::*;
use crate::v2::types::*;
use crate::hpack;
use crate::request::Request;
use crate::response::{Response, ResponseHandler};
use crate::v2::stream::Stream;

/// Volatile data associated with the connection.
pub struct ConnectionState {
    /// Whether or not run() was ever called on this Connection.
    /// This is mainly used to ensure that at most one set of reader/writer threads are associated with the connection.
    pub running: bool,

    // TODO: Need to have solid understanding of the GOAWAY state where we are gracefully shutting down but there was no error.
    // Likewise if we send a GOAWAY, we shoulnd't create new streams?? (e.g. no more requests or PUSH_PROMISES??)

    /// If present, then the 
    pub error: Option<ProtocolErrorV2>,

    // TODO: Shard this into the reader and writer states.

    /// Used to decode remotely created headers received on the connection.
    /// NOTE: This is shared across all streams on the connection.
    pub remote_header_decoder: hpack::Decoder,

    /// Settings currently in use by this endpoint.
    pub local_settings: SettingsContainer,

    /// Time at which the 'local_pending_settings' were sent to the remote server.
    /// A value of None means that no settings changes are pending.
    pub local_settings_sent_time: Option<DateTime<Utc>>,

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
    Goaway {
        last_stream_id: StreamId,
        error: ProtocolErrorV2
    },
    
    /// We received a remote GOAWAY with a non-NO_ERROR code or the reader thread failed, so we
    /// should close the connection ASAP.
    Closing,

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

    StreamWrite {
        stream_id: StreamId
    },

    /// We are an HTTP client connection and a locally generated request needs to be sent to the
    /// other endpoint.
    SendRequest {
        request: Request,
        response_handler: Box<dyn ResponseHandler>
    },

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
    }
}
