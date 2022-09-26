use common::errors::*;

use crate::request::Request;
use crate::response::Response;

#[derive(Default, Clone)]
pub struct ClientRequestContext {
    pub wait_for_ready: bool,
}

#[async_trait]
pub trait ClientInterface {
    async fn request(
        &self,
        request: Request,
        request_context: ClientRequestContext,
    ) -> Result<Response>;

    async fn current_state(&self) -> ClientState;
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ClientState {
    /// Initial state of the client.
    /// No attempt has been made yet to connect to a remote server so the health
    /// is still unknown but we should start connecting soon.
    Idle,

    Connecting,

    Ready,

    Failure,

    Shutdown,
}

impl ClientState {
    /// Returns whether or not the request should be instantly rejected (return
    /// an error).
    ///
    /// TODO: Use this for something.
    pub fn should_reject_request(&self, request_context: &ClientRequestContext) -> bool {
        match *self {
            ClientState::Idle => false,
            ClientState::Connecting => false,
            ClientState::Ready => false,
            ClientState::Failure => request_context.wait_for_ready,
            ClientState::Shutdown => true,
        }
    }
}

#[async_trait]
pub trait ClientEventListener: Send + Sync + 'static {
    async fn handle_client_state_change(&self);
}
