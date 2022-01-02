use std::collections::HashMap;
use std::future::Future;
use std::marker::PhantomData;
use std::sync::Arc;

use common::async_std::channel;
use common::bytes::Buf;
use common::bytes::Bytes;
use common::errors::*;
use common::io::Readable;
use common::task::ChildTask;
use common::CancellationToken;
use http::header::*;
use http::status_code::*;
use http::Body;

use crate::constants::GRPC_PROTO_TYPE;
use crate::message::*;
use crate::metadata::Metadata;
use crate::server_types::*;
use crate::service::Service;
use crate::status::*;
use crate::Channel;

pub struct Http2Server {
    handler: Http2RequestHandler,
    shutdown_token: Option<Box<dyn CancellationToken>>,
    start_callbacks: Vec<Box<dyn Fn() + Send + Sync + 'static>>,
}

impl Http2Server {
    pub fn new() -> Self {
        Self {
            handler: Http2RequestHandler {
                request_handlers: HashMap::new(),
                services: HashMap::new(),
            },
            shutdown_token: None,
            start_callbacks: vec![],
        }
    }

    pub fn add_service(&mut self, service: Arc<dyn Service>) -> Result<()> {
        let service_name = service.service_name().to_string();
        if self.handler.services.contains_key(&service_name) {
            return Err(err_msg("Adding duplicate service to RPCServer"));
        }

        self.handler.services.insert(service_name, service);
        Ok(())
    }

    pub fn add_request_handler<H: http::ServerHandler>(
        &mut self,
        path: &str,
        handler: H,
    ) -> Result<()> {
        // TODO: Also check for service conflicts?
        if self
            .handler
            .request_handlers
            .insert(path.to_string(), Box::new(handler))
            .is_some()
        {
            return Err(err_msg("Duplicate request handler mounted"));
        }

        Ok(())
    }

    pub fn add_start_callback<F: Fn() + Send + Sync + 'static>(&mut self, callback: F) {
        self.start_callbacks.push(Box::new(callback));
    }

    pub fn set_shutdown_token(&mut self, token: Box<dyn CancellationToken>) {
        self.shutdown_token = Some(token);
    }

    pub fn services(&self) -> impl Iterator<Item = &dyn Service> {
        self.handler.services.iter().map(|(_, v)| v.as_ref())
    }

    pub fn run(mut self, port: u16) -> impl Future<Output = Result<()>> + 'static {
        let mut options = http::ServerOptions::default();
        options.force_http2 = true;

        let mut server = http::Server::new(self.handler, options);
        if let Some(token) = self.shutdown_token.take() {
            server.set_shutdown_token(token);
        }

        while let Some(callback) = self.start_callbacks.pop() {
            callback();
        }

        server.run(port)
    }
}

/// NOTE: This is mainly pub(crate) to support the LocalChannel implementation.
/// TODO: Eventually make this private again.
pub(crate) struct Http2RequestHandler {
    request_handlers: HashMap<String, Box<dyn http::ServerHandler>>,

    services: HashMap<String, Arc<dyn Service>>,
}

impl Http2RequestHandler {
    pub(crate) fn new(service: Arc<dyn Service>) -> Self {
        let mut services = HashMap::new();
        services.insert(service.service_name().to_string(), service);

        Self {
            request_handlers: HashMap::new(),
            services,
        }
    }

    async fn handle_request_impl<'a>(
        &self,
        request: http::Request,
        context: http::ServerRequestContext<'a>,
    ) -> Result<http::Response> {
        // TODO: Convert as many of the errors in this function as possible to gRPC
        // trailing status codes.

        if let Some(request_handler) = self.request_handlers.get(request.head.uri.path.as_str()) {
            return Ok(request_handler.handle_request(request, context).await);
        }

        // Exit early if we detect a non-gRPC client
        // NOTE: According to the spec, this is the only time at whih we should ever
        // return a non-OK HTTP status. TODO: Technically anything starting with
        // "application/grpc" should be supported.
        if !request
            .head
            .headers
            .find_one(CONTENT_TYPE)
            .map(|v| v.value.as_bytes() == GRPC_PROTO_TYPE.as_bytes())
            .unwrap_or(false)
        {
            return Ok(http::ResponseBuilder::new()
                .status(http::status_code::UNSUPPORTED_MEDIA_TYPE)
                .build()
                .unwrap());
        }

        // NOTE: Returning an Err is not allowed before this point (to ensure that the
        // content type check goes through).

        // TODO: Should support different methods
        if request.head.method != http::Method::POST && request.head.method != http::Method::GET {
            return Ok(http::ResponseBuilder::new()
                .status(http::status_code::METHOD_NOT_ALLOWED)
                .build()
                .unwrap());
        }

        let request_context = ServerRequestContext {
            metadata: Metadata::from_headers(&request.head.headers)?,
        };

        let path_parts = request
            .head
            .uri
            .path
            .as_ref()
            .split('/')
            .collect::<Vec<_>>();
        if path_parts.len() != 3 || path_parts[0].len() != 0 {
            return Err(err_msg("Invalid path"));
        }

        let service_name = path_parts[1];
        let method_name = path_parts[2];
        let request = ServerStreamRequest::new(request.body, request_context);

        let response = self
            .handle_parsed_request(service_name, method_name, request)
            .await;

        Ok(response)
    }

    pub(crate) async fn handle_parsed_request(
        &self,
        service_name: &str,
        method_name: &str,
        request: ServerStreamRequest<()>,
    ) -> http::Response {
        match self
            .handle_parsed_request_impl(service_name, method_name, request)
            .await
        {
            Ok(r) => r,
            Err(e) => Self::error_response(e),
        }
    }

    async fn handle_parsed_request_impl(
        &self,
        service_name: &str,
        method_name: &str,
        request: ServerStreamRequest<()>,
    ) -> Result<http::Response> {
        let service = self
            .services
            .get(service_name)
            .ok_or(crate::Status::unimplemented(format!(
                "Unknown service named: {}",
                service_name
            )))?;

        let (response_sender, response_receiver) = channel::bounded(2);

        let child_task = ChildTask::spawn(Self::service_caller(
            service.clone(),
            method_name.to_string(),
            request,
            response_sender,
        ));

        let head_metadata = match response_receiver.recv().await? {
            ServerStreamResponseEvent::Head(metadata) => metadata,
            _ => {
                return Err(err_msg(
                    "Expected a head before other parts of the response",
                ));
            }
        };

        let response_builder = http::ResponseBuilder::new()
            .status(OK)
            .header(CONTENT_TYPE, GRPC_PROTO_TYPE);

        let body = Box::new(ResponseBody {
            child_task: Some(child_task),
            response_receiver,
            remaining_bytes: Bytes::new(),
            done_data: false,
            trailers: None,
        });

        let mut response = response_builder.body(body).build()?;
        head_metadata.append_to_headers(&mut response.head.headers)?;

        Ok(response)
    }

    /// Wrapper which calls the Service method for a single request and ensures
    /// that a trailer is eventually sent.
    async fn service_caller(
        service: Arc<dyn Service>,
        method_name: String,
        request: ServerStreamRequest<()>,
        response_sender: channel::Sender<ServerStreamResponseEvent>,
    ) {
        let mut response_context = ServerResponseContext::default();

        let mut head_sent = false;

        let response = ServerStreamResponse {
            phantom_t: PhantomData,
            context: &mut response_context,
            head_sent: &mut head_sent,
            sender: response_sender.clone(),
        };

        // TODO: If this fails with an error that can be downcast to a status, should we
        // propagate that back to the client.
        //
        // Probably no because this may imply that it was an internal RPC failure.
        // TODO: Ensure that similarly internal HTTP2 calls aren't propagated to
        // clients.
        let response_result = service.call(&method_name, request, response).await;

        if !head_sent {
            let _ = response_sender
                .send(ServerStreamResponseEvent::Head(
                    response_context.metadata.head_metadata,
                ))
                .await;
        }

        let _ = response_sender
            .send(ServerStreamResponseEvent::Trailers(
                response_result,
                response_context.metadata.trailer_metadata,
            ))
            .await;
    }

    /// Creates a simple http response from an Error
    ///
    /// NOTE: When failures occur before the service is called, the server won't
    /// return any head or trailer metadata.
    ///
    /// TODO: Consider eventually supporting the passing of metadata to enable
    /// tracing of RPCs.
    fn error_response(error: Error) -> http::Response {
        let (sender, receiver) = channel::bounded(1);
        sender
            .try_send(ServerStreamResponseEvent::Trailers(
                Err(error),
                Metadata::new(),
            ))
            .unwrap();

        // NOTE: GRPC servers are supported to always return 200 statuses.
        http::ResponseBuilder::new()
            .status(OK)
            .header(CONTENT_TYPE, GRPC_PROTO_TYPE)
            .body(Box::new(ResponseBody {
                child_task: None,
                response_receiver: receiver,
                remaining_bytes: Bytes::new(),
                done_data: false,
                trailers: None,
            }))
            .build()
            .unwrap()
    }
}

#[async_trait]
impl http::ServerHandler for Http2RequestHandler {
    async fn handle_request<'a>(
        &self,
        request: http::Request,
        context: http::ServerRequestContext<'a>,
    ) -> http::Response {
        match self.handle_request_impl(request, context).await {
            Ok(r) => r,
            Err(e) => Self::error_response(e),
        }
    }
}

struct ResponseBody {
    child_task: Option<ChildTask>,

    response_receiver: channel::Receiver<ServerStreamResponseEvent>,

    /// TODO: Re-use this buffer across multiple messages
    remaining_bytes: Bytes,

    /// If true, then we'll completely read all data.
    done_data: bool,

    trailers: Option<Headers>,
}

#[async_trait]
impl Readable for ResponseBody {
    async fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        loop {
            if !self.remaining_bytes.is_empty() {
                let n = std::cmp::min(self.remaining_bytes.len(), buf.len());
                buf[0..n].copy_from_slice(&self.remaining_bytes[0..n]);

                self.remaining_bytes.advance(n);

                // NOTE: We always stop after at least some amount of data is available to
                // ensure that readers are unblocked.
                return Ok(n);
            }

            if self.done_data {
                return Ok(0);
            }

            let event = self.response_receiver.recv().await?;
            match event {
                ServerStreamResponseEvent::Head(_) => {
                    return Err(err_msg("Unexpected head event"));
                }
                ServerStreamResponseEvent::Message(data) => {
                    // NOTE: This supports zero length packets are the message serializer will
                    // always prepend a fixed length prefix.
                    self.remaining_bytes = Bytes::from(MessageSerializer::serialize(&data));
                }
                ServerStreamResponseEvent::Trailers(result, trailer_meta) => {
                    let mut trailers = Headers::new();
                    trailer_meta.append_to_headers(&mut trailers)?;

                    match result {
                        Ok(()) => {
                            Status::ok().append_to_headers(&mut trailers)?;
                        }
                        Err(error) => {
                            // TODO: Have some default error handler to log the raw errors.
                            // TODO: Only forward statuses that were generated locally and not ones
                            // that were returned as part of an internal client RPC call.

                            eprintln!("RPC Error: {:?}", error);
                            let status = match error.downcast_ref::<Status>() {
                                Some(s) => s.clone(),
                                None => Status::internal("Internal error occured"),
                            };

                            status.append_to_headers(&mut trailers)?;
                        }
                    }

                    self.done_data = true;
                    self.trailers = Some(trailers);
                }
            }
        }
    }
}

#[async_trait]
impl Body for ResponseBody {
    fn len(&self) -> Option<usize> {
        None
    }

    fn has_trailers(&self) -> bool {
        true
    }

    async fn trailers(&mut self) -> Result<Option<Headers>> {
        if !(self.done_data && self.remaining_bytes.is_empty() && self.trailers.is_some()) {
            return Err(err_msg("Trailers read at the wrong time"));
        }

        Ok(self.trailers.take())
    }
}
