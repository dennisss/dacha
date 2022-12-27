use std::sync::Arc;

use executor::channel;
use executor::child_task::ChildTask;
use executor::sync::Mutex;

use crate::hpack;
use crate::method::Method;
use crate::proto::v2::*;
use crate::request::Request;
use crate::response::{Response, ResponseHandler};
use crate::v2::body::*;
use crate::v2::stream_state::*;
use crate::v2::types::*;

/// Representation of an HTTP2 stream.
///
/// Stream objects are only created for non-idle streams.
/// Stream objects are owned by the ConnectionState object.  
pub struct Stream {
    /// Internal state variables used by multiple threads.
    pub state: Arc<Mutex<StreamState>>,

    /// Used to let the IncomingStreamBody know that data is available to be
    /// read.
    ///
    /// MUST be called whenever any of the following fields are changed:
    /// - state.receiving_buffer
    /// - state.receiving_end
    /// - state.error
    pub read_available_notifier: channel::Sender<()>,

    /// Used to let the local thread that is processing this stream know that
    /// more data can be written to the stream.
    ///
    /// MUST be called whenever any of the following fields are changed:
    /// - state.remote_window
    /// - state.sending_buffer (not in OutgoingStreamBody)
    pub write_available_notifier: channel::Sender<()>,

    /// If not None, then this stream was used to send a request to a remote
    /// server and we are currently waiting for the response headers to
    /// become available.
    pub incoming_response_handler: Option<(Method, Box<dyn ResponseHandler>, IncomingStreamBody)>,

    pub outgoing_response_handler: Option<(Method, OutgoingStreamBody)>,

    /// Whether or not the writer thread has written a packet with end_of_stream
    /// flag yet.
    /// This is needed to ensure that we can tell if an empty outgoing body
    /// has already been communicated to the other side.
    pub sending_end_flushed: bool,

    /// Tasks used to process this stream. Specifically we use tasks for:
    /// - Computing/sending the body to be sent to the other endpoint.
    /// - For servers, we spawn a new task for generating the response that
    ///   should be sent to the client.
    ///
    /// We retain handles to these so that we can cancel them should we need to
    /// abruptly close the stream due to protocol level errors.
    ///
    /// TODO: Ensure that this is ALWAYS cancelled when the stream or connection
    /// is garbage collected.
    pub processing_tasks: Vec<ChildTask>,
}

impl Stream {
    pub fn is_closed(&self, state: &StreamState) -> bool {
        if state.error.is_some() {
            return true;
        }

        // state.reader_closed is a special case which will cause us to sent a
        // RST_STREAM with a CANCELLED error in finish_stream.
        self.sending_end_flushed && (state.received_end || state.reader_closed)
    }

    pub fn remote_window(&self, state: &mut StreamState) -> WindowSize {
        state.remote_window
    }

    pub fn set_remote_window(&self, state: &mut StreamState, value: WindowSize) {
        state.remote_window = value;
        let _ = self.write_available_notifier.try_send(());
    }

    fn set_received_end(&self, end_stream: bool, state: &mut StreamState) -> StreamResult<()> {
        if state.received_end {
            return Err(StreamError::stream_closed(
                "Received data after stream was closed",
            ));
        }

        state.received_end = end_stream;
        Ok(())
    }

    /// NOTE: This is expected to be called after set_end_stream
    fn increment_received_bytes(
        &self,
        increment: usize,
        state: &mut StreamState,
    ) -> StreamResult<()> {
        if let Some(expected_size) = state.received_expected_bytes.clone() {
            // NOTE: We only increment this if we expect a bounded length so
            // that we can support infinite lenth streams.
            state.received_total_bytes = state
                .received_total_bytes
                .checked_add(increment)
                .ok_or_else(|| {
                    StreamError::malformed_message("Overflow in received number of bytes")
                })?;

            let good = {
                if state.received_end {
                    expected_size == state.received_total_bytes
                } else {
                    expected_size >= state.received_total_bytes
                }
            };

            if !good {
                return Err(StreamError::malformed_message(
                    "Mismatch between received and expected stream length",
                ));
            }
        }

        Ok(())
    }

    pub fn receive_request(
        &self,
        headers: Vec<hpack::HeaderField>,
        end_stream: bool,
        incoming_body: IncomingStreamBody,
        state: &mut StreamState,
    ) -> Option<Request> {
        if state.error.is_some() {
            return None;
        }

        match self.receive_request_inner(headers, end_stream, incoming_body, state) {
            Ok(r) => Some(r),
            Err(StreamError(error)) => {
                state.error = Some(error);
                None
            }
        }
    }

    /// Called on a server when we just got request headers from the client.
    fn receive_request_inner(
        &self,
        headers: Vec<hpack::HeaderField>,
        end_stream: bool,
        incoming_body: IncomingStreamBody,
        state: &mut StreamState,
    ) -> StreamResult<Request> {
        // TODO: These may cause the stream to immediately fail.
        let head = crate::v2::headers::process_request_head(headers)?;
        let body = decode_request_body_v2(&head, incoming_body, state)?;

        let request = Request { head, body };

        // NOTE: This may fail if we are simultaneously ending the remote end
        // of the stream and receiving a body with a non-zero Content-Length
        self.receive_data_inner(&[], 0, end_stream, state)?;

        Ok(request)
    }

    pub fn receive_response(
        &self,
        request_method: Method,
        headers: Vec<hpack::HeaderField>,
        end_stream: bool,
        incoming_body: IncomingStreamBody,
        state: &mut StreamState,
    ) -> Option<Response> {
        if state.error.is_some() {
            return None;
        }

        match self.receive_response_inner(request_method, headers, end_stream, incoming_body, state)
        {
            Ok(r) => Some(r),
            Err(StreamError(error)) => {
                state.error = Some(error);
                None
            }
        }
    }

    fn receive_response_inner(
        &self,
        request_method: Method,
        headers: Vec<hpack::HeaderField>,
        end_stream: bool,
        incoming_body: IncomingStreamBody,
        state: &mut StreamState,
    ) -> StreamResult<Response> {
        let head = crate::v2::headers::process_response_head(headers)?;
        let body =
            crate::v2::body::decode_response_body_v2(request_method, &head, incoming_body, state)?;

        let response = Response { head, body };

        self.receive_data_inner(&[], 0, end_stream, state)?;

        Ok(response)
    }

    // TODO: We must ensure that these functions immediately store an error in the
    // stream state so that we don't run into any race conditions with the
    // incoming/outgoing body checking the error in between locking cycles.

    pub fn receive_data(
        &self,
        data: &[u8],
        extra_flow_controlled_bytes: usize,
        end_stream: bool,
        state: &mut StreamState,
    ) {
        if state.error.is_some() {
            return;
        }

        if let Err(StreamError(error)) =
            self.receive_data_inner(data, extra_flow_controlled_bytes, end_stream, state)
        {
            // TODO: Should I notify the body reader/writers?

            state.error = Some(error);
        }
    }

    /// Called whenever we successfully received a DATA or trailing HEADERS
    /// frame.
    ///
    /// If this fails, then the stream needs to be reset with the returned
    /// error.
    fn receive_data_inner(
        &self,
        data: &[u8],
        extra_flow_controlled_bytes: usize,
        end_stream: bool,
        state: &mut StreamState,
    ) -> StreamResult<()> {
        self.set_received_end(end_stream, state)?;

        if self.incoming_response_handler.is_some() {
            return Err(StreamError::malformed_message(
                "Expected response headers before stream data",
            ));
        }

        // NOTE: This must run before we add the data to the received_buffer to ensure
        // that we don't give the user bad data (e.g. a smuggled request).
        self.increment_received_bytes(data.len(), state)?;

        if state.local_window < ((data.len() + extra_flow_controlled_bytes) as WindowSize) {
            // Send a RST_STREAM
            return Err(StreamError(ProtocolErrorV2 {
                code: ErrorCode::FLOW_CONTROL_ERROR,
                message: "Received data frame exceeded flow control window size",
                local: true,
            }));
        }
        state.local_window -= (data.len() + extra_flow_controlled_bytes) as WindowSize;

        if !state.reader_closed {
            state.received_buffer.extend_from_slice(&data);

            // Notify the IncomingStreamBody if there was a change.
            if !data.is_empty() || end_stream {
                let _ = self.read_available_notifier.try_send(());
            }
        }

        Ok(())
    }

    pub fn receive_trailers(
        &self,
        trailers: Vec<hpack::HeaderField>,
        end_stream: bool,
        state: &mut StreamState,
    ) {
        if state.error.is_some() {
            return;
        }

        if let Err(StreamError(error)) = self.receive_trailers_inner(trailers, end_stream, state) {
            state.error = Some(error);
        }
    }

    fn receive_trailers_inner(
        &self,
        trailers: Vec<hpack::HeaderField>,
        end_stream: bool,
        state: &mut StreamState,
    ) -> StreamResult<()> {
        self.set_received_end(end_stream, state)?;

        if self.incoming_response_handler.is_some() {
            return Err(StreamError::malformed_message(
                "Expected response headers before trailer headers",
            ));
        }

        // NOTE: This may fail as we need to verify '==' of the received bytes once the
        // stream has ended.
        self.increment_received_bytes(0, state)?;

        if !end_stream {
            // Error defined in 'RFC 7540: Section 8.1'
            return Err(StreamError::malformed_message(
                "Received trailers for request without closing stream",
            ));
        }

        state.received_trailers = Some(crate::v2::headers::process_trailers(trailers)?);

        let _ = self.read_available_notifier.try_send(());

        Ok(())
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
