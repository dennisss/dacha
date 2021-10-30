use common::errors::*;

use crate::request::Request;
use crate::response::Response;

#[async_trait]
pub trait ClientInterface {
    async fn request(&self, mut request: Request) -> Result<Response>;
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
