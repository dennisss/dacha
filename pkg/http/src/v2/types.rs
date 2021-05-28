use crate::proto::v2::ErrorCode;

pub type StreamId = u32;

/// Type used to represent the size of the flow control window.
///
/// NOTE: The window may go negative.
pub type WindowSize = i32;


// TODO: Distinguish between locally created errors vs remotely created errors.
#[derive(Debug, Clone, Fail)]
pub struct ProtocolErrorV2 {
    pub code: ErrorCode,

    /// NOTE: This message should only contain non-sensitive data that can be safely
    /// sent to the other endpoint.
    pub message: &'static str,
    
    /// If true, this error was generated locally rather than being received from
    /// the remote endpoint.
    pub local: bool,
}

impl ProtocolErrorV2 {
    /// In the context of a request sent from a client to a server, this indicates
    /// whether or not the client is safe to retry the request because no application
    /// level processing was started on the request.
    pub fn is_retryable(&self) -> bool {
        self.code == ErrorCode::REFUSED_STREAM
    }
}

impl std::fmt::Display for ProtocolErrorV2 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}: [{}] {}", self.code, if self.local { "LOCAL" } else { "REMOTE" }, self.message)
    }
}

pub type ProtocolResultV2<T> = std::result::Result<T, ProtocolErrorV2>;