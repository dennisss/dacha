use common::errors::*;

use crate::request::Request;
use crate::response::Response;

#[derive(Default)]
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
    NotConnected,

    Connecting,

    Connected,

    Failure,
}
