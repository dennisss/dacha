use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use net::ip::IPAddress;

use crate::request::Request;
use crate::response::Response;

pub type ServerConnectionId = u64;

/// TODO: Rename this 'Service'?
/// TODO: Add a separate RequestHandler trait to enable having objects which can
/// re-write requests but don't care about life-cycle.
#[async_trait]
pub trait ServerHandler: 'static + Send + Sync {
    /// Called whenever a new connection is started but before any requests are
    /// issued (aka after TCP/TLS but before HTTP negotation).
    ///
    /// Returns whether or not we should continue running the connection.
    async fn handle_connection(&self, _context: &ServerConnectionContext) -> bool {
        true
    }

    /// Processes an HTTP request returning a response eventually.
    ///
    /// While the full request is available in the first argument, the following
    /// headers are handled automatically in the server:
    /// - Content-Length
    /// - Transfer-Encoding
    /// - Connection
    /// - Keep-Alive
    /// - TE
    /// - Host
    async fn handle_request<'a>(
        &self,
        request: Request,
        context: ServerRequestContext<'a>,
    ) -> Response;
}

#[async_trait]
impl<T: ServerHandler> ServerHandler for Arc<T> {
    async fn handle_connection(&self, context: &ServerConnectionContext) -> bool {
        self.as_ref().handle_connection(context).await
    }

    async fn handle_request<'a>(
        &self,
        request: Request,
        context: ServerRequestContext<'a>,
    ) -> Response {
        self.as_ref().handle_request(request, context).await
    }
}

/// General information about a connection to a server (a single connection may
/// be re-used by multiple requests).
#[derive(Clone, Debug)]
pub struct ServerConnectionContext {
    /// Unique id for this connection.
    pub id: ServerConnectionId,

    pub peer_addr: IPAddress,

    pub peer_port: u16,

    pub tls: Option<crypto::tls::HandshakeSummary>,
}

/// Metadata about the incoming request.
#[derive(Clone, Debug)]
pub struct ServerRequestContext<'a> {
    pub connection_context: &'a ServerConnectionContext,
    /* TODO: For HTTP2 connections, support issuing server pushes. */
}

/// Wraps a simple static function as a server request handler.
/// See ServerHandler::handle_request for more information.
pub fn HttpFn<
    F: Future<Output = Response> + Send + 'static,
    H: (Fn(Request) -> F) + Send + Sync + 'static,
>(
    handler_fn: H,
) -> HandleRequestFnWrap {
    HandleRequestFnWrap {
        value: Box::new(move |req| Box::pin(handler_fn(req))),
    }
}

/// Internal: Used by HttpFn.
pub struct HandleRequestFnWrap {
    value: Box<dyn (Fn(Request) -> Pin<Box<dyn Future<Output = Response> + Send>>) + Send + Sync>,
}

#[async_trait]
impl ServerHandler for HandleRequestFnWrap {
    async fn handle_request<'a>(
        &self,
        request: Request,
        _context: ServerRequestContext<'a>,
    ) -> Response {
        (self.value)(request).await
    }
}
