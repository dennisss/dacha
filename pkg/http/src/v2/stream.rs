use std::sync::Arc;

use common::async_std::task;
use common::async_std::channel;
use common::async_std::sync::Mutex;
use common::task::ChildTask;

use crate::response::ResponseHandler;
use crate::v2::body::*;
use crate::v2::stream_state::*;

/// Representation of an HTTP2 stream.
///
/// Stream objects are only created for non-idle streams.
/// Streams are owned by the ConnectionState object.  
pub struct Stream {
    /// Internal state variables used by multiple threads.
    pub state: Arc<Mutex<StreamState>>,

    /// Used to let the IncomingStreamBody know that data is available to be read.
    pub read_available_notifier: channel::Sender<()>,

    /// Used to let the local thread that is processing this stream know that
    /// more data can be written to the stream.
    pub write_available_notifier: channel::Sender<()>,

    /// If not None, then this stream was used to send a request to a remote server and we are
    /// currently waiting for the response headers to become available.
    pub incoming_response_handler: Option<(Box<dyn ResponseHandler>, IncomingStreamBody)>,

    pub outgoing_response_handler: Option<OutgoingStreamBody>,

    /// Whether or not the writer thread has written a packet with end_of_stream
    /// flag yet.
    /// This is needed to ensure that we can tell if an empty outgoing body
    /// has already been communicated to the other side.
    pub sent_end_of_stream: bool,

    /// Tasks used to process this stream. Specifically we use tasks for:
    /// - Computing/sending the body to be sent to the other endpoint.
    /// - For servers, we spawn a new task for generating the response that should be sent to the
    ///   client.
    ///
    /// We retain handles to these so that we can cancel them should we need to abruptly close the
    /// stream due to protocol level errors.
    ///
    /// TODO: Ensure that this is ALWAYS cancelled when the stream or connection is garbage collected.
    pub processing_tasks: Vec<ChildTask>,
}

impl Stream {
    /// Called whenever we successfully received a DATA frame or another frame
    /// that has an END_STREAM flag.
    pub fn receive_data(&self, data: &[u8], end_stream: bool, state: &mut StreamState) {
        state.received_end_of_stream = end_stream;
        state.received_buffer.extend_from_slice(&data);

        // Notify the IncomingStreamBody if there was a change.
        if !data.is_empty() || end_stream {
            let _ = self.read_available_notifier.try_send(());
        }
    } 
}

/*
Representing priority:
- We'll measure network usage over 1 second.

    // TODO: Priorities can be assigned to idle/unused tasks, so we shouldn't necessarily associate
    // it with the stream. 
    pub weight: u8,

    pub dependency: StreamId,

*/

