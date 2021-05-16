use std::sync::Arc;

use common::async_std::task;
use common::async_std::channel;
use common::async_std::sync::Mutex;

use crate::response::ResponseHandler;
use crate::v2::body::*;
use crate::v2::stream_state::*;

/// Representation of an HTTP2 stream.
///
/// NOTE: Stream objects are only created for non-idle streams. 
pub struct Stream {
    /// Internal state variables used by multiple threads.
    pub state: Arc<Mutex<StreamState>>,

    /// Used to let the body object know that data is available to be read.
    pub read_available_notifier: channel::Sender<()>,

    /// Used to let the local thread that is processing this stream know that
    /// more data can be written to the stream.
    pub write_available_notifier: channel::Sender<()>,

    /// If not None, then this stream was used to send a request to a remote server and we are
    /// currently waiting for the response headers to become available.
    pub incoming_response_handler: Option<(Box<dyn ResponseHandler>, IncomingStreamBody)>,

    pub outgoing_response_handler: Option<OutgoingStreamBody>,

    /// Tasks used to process this stream. Specifically we use tasks for:
    /// - Computing/sending the body to be sent to the other endpoint.
    /// - For servers, we spawn a new task for generating the response that should be sent to the
    ///   client.
    ///
    /// We retain handles to these so that we can cancel them should we need to abruptly close the
    /// stream due to protocol level errors.
    ///
    /// TODO: Ensure that this is ALWAYS cancelled when the stream or connection is garbage collected.
    pub processing_tasks: Vec<task::JoinHandle<()>>,
}

/*
Representing priority:
- We'll measure network usage over 1 second.

    // TODO: Priorities can be assigned to idle/unused tasks, so we shouldn't necessarily associate
    // it with the stream. 
    pub weight: u8,

    pub dependency: StreamId,

*/

