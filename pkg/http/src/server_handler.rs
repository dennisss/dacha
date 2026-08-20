use std::any::Any;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use common::errors::*;
use common::io::*;
use net::ip::{IPAddress, SocketAddr};

use crate::request::{Request, RequestHead};
use crate::response::Response;

pub type ServerConnectionId = u64;

/// TODO: Rename this 'Service'?
/// TODO: Add a separate RequestHandler trait to enable having objects which can
/// re-write requests but don't care about life-cycle.
#[async_trait]
pub trait ServerHandler: 'static + Send + Sync {

    /// Called when a new TCP stream has been opened but before
    /// any processing (e.g. TCP handshaking has started).
    ///
    /// Returns whether or not we should continue running the connection.
    fn handle_connecting(&self, _context: &mut ServerConnectionContext) -> bool {
        true
    }

    /// Called whenever a new connection is started but before any requests are
    /// issued (aka after TCP/TLS but before HTTP negotation).
    ///
    /// Returns whether or not we should continue running the connection.
    async fn handle_connection(&self, _context: &mut ServerConnectionContext) -> bool {
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

    async fn handle_upgrade(
        &self,
        request_head: RequestHead,
        reader: Box<dyn Readable>,
        writer: Box<dyn SharedWriteable>
    ) -> Result<()> {
        // Default drop it.
        Ok(())
    }
}

#[async_trait]
impl<T: ServerHandler> ServerHandler for Arc<T> {
    fn handle_connecting(&self, context: &mut ServerConnectionContext) -> bool {
        self.as_ref().handle_connecting(context)
    }

    async fn handle_connection(&self, context: &mut ServerConnectionContext) -> bool {
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
///
/// TODO: Disallow mutating anything other than the handler_data.
#[derive(Clone, Debug)]
pub struct ServerConnectionContext {
    /// Unique id for this connection.
    pub id: ServerConnectionId,

    pub peer: SocketAddr,

    /// If set, the connection was made over TLS with the given metadata
    /// produced during the handshake.
    pub tls: Option<crypto::tls::HandshakeSummary>,

    /// Server specific connection wide data populated by
    /// 'ServerHandler::handle_connection'.
    pub handler_data: Option<Arc<dyn Any + Send + Sync>>,
}

/// Metadata about the incoming request.
#[derive(Clone, Debug)]
pub struct ServerRequestContext<'a> {
    pub connection_context: &'a ServerConnectionContext,

    /// Optional data added by a `ServerHandler::handle_request` call before
    /// calling into a nested handle_request call.
    pub handler_data: Option<Arc<dyn Any + Send + Sync>>,

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
