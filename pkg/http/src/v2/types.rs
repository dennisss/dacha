use common::async_std::channel;
use common::errors::Result;

use crate::proto::v2::ErrorCode;
use crate::response::Response;


pub type StreamId = u32;

/// Type used to represent the size of the flow control window.
///
/// NOTE: The window may go negative.
pub type WindowSize = i32;


// TODO: Distinguish between locally created errors vs remotely created errors.
#[derive(Debug, Clone, Fail)]
pub struct ProtocolError {
    pub code: ErrorCode,
    pub message: &'static str,
    
    /// If true, this error was generated locally rather than being received from
    /// the remote endpoint.
    pub local: bool,
}

impl ProtocolError {
    /// In the context of a request sent from a client to a server, this indicates
    /// whether or not the client is safe to retry the request because no application
    /// level processing was started on the request.
    pub fn is_retryable(&self) -> bool {
        self.code == ErrorCode::REFUSED_STREAM
    }
}

impl std::fmt::Display for ProtocolError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}: [{}] {}", self.code, if self.local { "LOCAL" } else { "REMOTE" }, self.message)
    }
}

pub type ProtocolResult<T> = std::result::Result<T, ProtocolError>;

#[async_trait]
pub trait ResponseHandler: Send + Sync {
    // TODO: Document whether or not this should be a 'fast' running function. This will determine
    // whether or not we need to spawn a new task in the connection code to run it.
    async fn handle_response(&self, response: Result<Response>);
}

#[async_trait]
impl ResponseHandler for channel::Sender<Result<Response>> {
    async fn handle_response(&self, response: Result<Response>) {
        let _ = self.send(response).await;
    }
}