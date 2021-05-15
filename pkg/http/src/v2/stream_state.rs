use crate::v2::types::*;

/// Variable state associated with the stream.
/// NOTE: 
/// TODO: Split into reader and writer states 
pub struct StreamState {
    /// Error state of the stream. If present, then this stream was abruptly closed.
    ///
    /// This corresponds to either a local/remote RST_STREAM or GOAWAY frame being sent.
    pub error: Option<ProtocolError>,

    /// Number of bytes of data the local endpoint is willing to accept from the remote endpoint for
    /// this stream. 
    pub local_window: WindowSize,

    /// Data which has been received from the remote endpoint as part of DATA frames but hasn't
    /// been read by the stream handler yet.
    ///
    /// TODO: Make this a cyclic buffer or a list of chunked buffers. (the challenge with a cyclic
    /// buffer is that we should block accidentally overriding data)
    pub received_buffer: Vec<u8>,

    /// If true, aside from what is in 'received_buffer', we have received all data on this stream from
    /// the remote endpoint.
    ///
    /// TODO: Support non-data trailers?
    pub received_end_of_stream: bool,

    /// Number of bytes that we expect to be read from the stream.
    /// Derived from the 'Content-Length' header received if any.
    pub received_expected_bytes: Option<usize>,

    /// Total number of bytes that we've received from the remote endpoint on this stream.
    pub received_total_bytes: usize,

    /// Number of bytes the remote endpoint is willing to accept from the local endpoint for
    /// this stream.
    pub remote_window: WindowSize,

    /// Data waiting to be sent to the remote endpoint.
    /// TODO: Need to be sinegat restrictive about how big this can get (can't use remote_window as the
    /// max for this as that may be an insanely large number)
    pub sending_buffer: Vec<u8>,

    /// If true, 'sending_buffer' contains the last remaining data that needs to be sent through
    /// this stream.
    pub sending_at_end: bool,
}

impl StreamState {
    // pub fn is_closed(&self) -> bool {

    // }

}
