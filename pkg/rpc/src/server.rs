use std::collections::HashMap;
use std::future::Future;
use std::io::Cursor;
use std::marker::PhantomData;
use std::sync::Arc;

use common::bytes::Buf;
use common::bytes::Bytes;
use common::errors::*;
use common::io::Readable;
use executor::cancellation::CancellationToken;
use executor::channel::spsc;
use executor::child_task::ChildTask;
use executor_multitask::ServiceResource;
use http::header::*;
use http::status_code::*;
use http::Body;

use crate::buffer_queue::BufferQueue;
use crate::buffer_queue::BufferQueueCursor;
use crate::media_type::RPCMediaProtocol;
use crate::media_type::RPCMediaType;
use crate::message::*;
use crate::metadata::Metadata;
use crate::server_types::*;
use crate::service::Service;
use crate::status::*;
use crate::Channel;

type StartCallback = Box<dyn FnOnce(Arc<dyn ServiceResource>) + Send + Sync + 'static>;

/// RPC server implemented on top of an HTTP2 server.
pub struct Http2Server {
    handler: Http2RequestHandler,
    start_callbacks: Vec<StartCallback>,
    allow_http1: bool,
    port: Option<u16>,
}

impl Http2Server {
    pub fn new(port: Option<u16>) -> Self {
        Self {
            handler: Http2RequestHandler {
                request_handlers: HashMap::new(),
                services: HashMap::new(),
                codec_options: Arc::new(ServerCodecOptions::default()),
                enable_cors: false,
            },
            start_callbacks: vec![],
            allow_http1: false,
            port,
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

    /// Adds a callback which will be executed when the RPC server has started
    /// loading.
    pub fn add_start_callback<F: FnOnce(Arc<dyn ServiceResource>) + Send + Sync + 'static>(
        &mut self,
        callback: F,
    ) {
        self.start_callbacks.push(Box::new(callback));
    }
    pub fn enable_cors(&mut self) {
        self.handler.enable_cors = true;
    }

    pub fn allow_http1(&mut self) {
        self.allow_http1 = true;
    }

    pub fn codec_options_mut(&mut self) -> &mut ServerCodecOptions {
        Arc::get_mut(&mut self.handler.codec_options).unwrap()
    }

    pub fn services(&self) -> impl Iterator<Item = &dyn Service> {
        self.handler.services.iter().map(|(_, v)| v.as_ref())
    }

    fn to_inner_server(self) -> (http::Server, Vec<StartCallback>) {
        let mut options = http::ServerOptions::default();
        options.force_http2 = !self.allow_http1;
        options.port = self.port;
        options.name = "rpc::Http2Server".to_string();
        (
            http::Server::new(self.handler, options),
            self.start_callbacks,
        )
    }

    pub async fn bind(self) -> Result<BoundHttp2Server> {
        let (server, start_callbacks) = self.to_inner_server();
        let bound_http_server = server.bind().await?;
        Ok(BoundHttp2Server {
            bound_http_server,
            start_callbacks,
        })
    }

    pub fn start(self) -> Arc<dyn ServiceResource> {
        let (server, start_callbacks) = self.to_inner_server();
        let r = Arc::new(server.start());
        for c in start_callbacks {
            c(r.clone())
        }

        r
    }
}

pub struct BoundHttp2Server {
    bound_http_server: http::BoundServer,
    start_callbacks: Vec<StartCallback>,
}

impl BoundHttp2Server {
    pub fn local_addr(&self) -> Result<net::ip::SocketAddr> {
        self.bound_http_server.local_addr()
    }

    pub fn start(self) -> Arc<dyn ServiceResource> {
        let r = Arc::new(self.bound_http_server.start());
        for c in self.start_callbacks {
            c(r.clone())
        }

        r
    }
}

/// Implementation of the HTTP2 request handler for processing RPC requests.
///
/// NOTE: This is mainly pub(crate) to support the LocalChannel implementation.
/// TODO: Eventually make this private again.
pub(crate) struct Http2RequestHandler {
    request_handlers: HashMap<String, Box<dyn http::ServerHandler>>,

    services: HashMap<String, Arc<dyn Service>>,

    pub(crate) codec_options: Arc<ServerCodecOptions>,

    enable_cors: bool,
}

impl Http2RequestHandler {
    pub(crate) fn new(service: Arc<dyn Service>, enable_cors: bool) -> Self {
        let mut services = HashMap::new();
        services.insert(service.service_name().to_string(), service);

        Self {
            request_handlers: HashMap::new(),
            enable_cors,
            codec_options: Arc::new(ServerCodecOptions::default()),
            services,
        }
    }

    async fn handle_request_impl<'a>(
        &self,
        request: http::Request,
        context: http::ServerRequestContext<'a>,
    ) -> http::Response {
        // TODO: Need to start thinking of this multi-dimensionally (Host, Path, Method)
        if let Some(request_handler) = self.request_handlers.get(request.head.uri.path.as_str()) {
            return request_handler.handle_request(request, context).await;
        }

        if self.enable_cors && request.head.method == http::Method::OPTIONS {
            return http::ResponseBuilder::new()
                .status(http::status_code::NO_CONTENT)
                .header(CACHE_CONTROL, "max-age=600")
                .build()
                .unwrap();
        }

        // Exit early if we detect a non-gRPC client
        // NOTE: According to the spec, this is the only time at whih we should ever
        // return a non-OK HTTP status. TODO: Technically anything starting with
        // "application/grpc" should be supported.
        let request_type = match RPCMediaType::parse(&request.head.headers) {
            Some(v) => v,
            None => {
                return http::ResponseBuilder::new()
                    .status(http::status_code::UNSUPPORTED_MEDIA_TYPE)
                    .build()
                    .unwrap();
            }
        };

        match self
            .handle_request_impl_result(request, request_type.clone(), context)
            .await
        {
            Ok(r) => r,
            // Use same response type as request type.
            Err(e) => Self::error_response(e, request_type),
        }
    }

    async fn handle_request_impl_result<'a>(
        &self,
        request: http::Request,
        request_type: RPCMediaType,
        context: http::ServerRequestContext<'a>,
    ) -> Result<http::Response> {
        // TODO: Convert as many of the errors in this function as possible to gRPC
        // trailing status codes.

        // Use the same type to respond to the request as the request type.
        // TODO: Examine the "Accept" request header to tell which type the client
        // wants.
        let response_type = request_type.clone();

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
            // TODO: Convert to a grpc error.
            return Err(err_msg("Invalid path"));
        }

        let service_name = path_parts[1];
        let method_name = path_parts[2];
        let request = ServerStreamRequest::new(
            request.body,
            request_type,
            self.codec_options.clone(),
            request_context,
        );

        let response = self
            .handle_parsed_request(service_name, method_name, request, response_type)
            .await;

        Ok(response)
    }

    pub(crate) async fn handle_parsed_request(
        &self,
        service_name: &str,
        method_name: &str,
        request: ServerStreamRequest<()>,
        response_type: RPCMediaType,
    ) -> http::Response {
        match self
            .handle_parsed_request_impl(service_name, method_name, request, response_type.clone())
            .await
        {
            Ok(r) => r,
            Err(e) => Self::error_response(e, response_type),
        }
    }

    async fn handle_parsed_request_impl(
        &self,
        service_name: &str,
        method_name: &str,
        request: ServerStreamRequest<()>,
        response_type: RPCMediaType,
    ) -> Result<http::Response> {
        // TODO: Add a unit test for getting this error!
        let service = self
            .services
            .get(service_name)
            .ok_or(crate::Status::unimplemented(format!(
                "Unknown service named: {}",
                service_name
            )))?;

        // NOTE: This must be bounded to provide backpressure while waiting for the
        // connection to transfer the bytes.
        //
        // We must minimally have 3 slots (for a Head, Message, and Trailers)
        let (response_sender, response_receiver) = spsc::bounded(3);

        // This will call the actual service method and run until the entire contents of
        // the body are generated.
        let child_task = ChildTask::spawn(Self::service_caller(
            service.clone(),
            method_name.to_string(),
            request,
            response_sender,
            response_type.clone(),
            self.codec_options.clone(),
        ));

        let mut response_builder = http::ResponseBuilder::new()
            .status(OK)
            .header(CONTENT_TYPE, response_type.to_string());

        let mut body = Box::new(ResponseBody {
            child_task: Some(child_task),
            response_receiver,
            response_type,
            buffer: BufferQueue::new(),
            buffer_cursor: BufferQueueCursor::default(),
            done_data: false,
            trailers: None,
        });

        // TODO: Verify that we can receie this while sending END_STREAM for an
        // immediate response.

        // Wait until we get at least enough information to generate the response head.
        match body.response_receiver.recv().await? {
            ServerStreamResponseEvent::Head(head_metadata) => {
                head_metadata.append_to_headers(response_builder.headers())?;
            }
            ServerStreamResponseEvent::TrailersOnly(result, metadata) => {
                metadata
                    .head_metadata
                    .append_to_headers(response_builder.headers())?;
                metadata
                    .trailer_metadata
                    .append_to_headers(response_builder.headers())?;

                ResponseBody::append_result_to_headers(result, &mut response_builder.headers());

                // Immediately indicate that there will be no more data.
                body.done_data = true;
            }
            _ => {
                return Err(err_msg(
                    "Expected a head before other parts of the response",
                ));
            }
        };

        // Immediately apply any already prepared events.
        // This is mainly to enable proper functionality of corked events.
        for _ in 0..body.response_receiver.capacity() {
            if let Some(Ok(event)) = body.response_receiver.try_recv() {
                body.process_event(event)?;
            } else {
                break;
            }
        }

        // TODO: We also need HTTP2 layer support for

        // TODO: If
        let mut response = response_builder.body(body).build()?;

        Ok(response)
    }

    /// Wrapper which calls the Service method for a single request and ensures
    /// that a trailer is eventually sent.
    async fn service_caller(
        service: Arc<dyn Service>,
        method_name: String,
        request: ServerStreamRequest<()>,
        mut response_sender: spsc::Sender<ServerStreamResponseEvent>,
        response_type: RPCMediaType,
        codec_options: Arc<ServerCodecOptions>,
    ) {
        let mut response_context = ServerResponseContext::default();

        let mut head_sent = false;

        let response = ServerStreamResponse {
            phantom_t: PhantomData,
            context: &mut response_context,
            response_type,
            codec_options,
            head_sent: &mut head_sent,
            sender: &mut response_sender,
        };

        // TODO: If this fails with an error that can be downcast to a status, should we
        // propagate that back to the client.
        //
        // Probably no because this may imply that it was an internal RPC failure.
        // TODO: Ensure that similarly internal HTTP2 calls aren't propagated to
        // clients.
        let response_result = service.call(&method_name, request, response).await;

        // TODO: Uncork the channel to ensure that these can go through? (unless already
        // intentionally corked to batch the trailers).

        if !head_sent {
            // Trailers-Only case.
            // If a head wasn't sent, then that implies there was no data either.
            let _ = response_sender
                .send(ServerStreamResponseEvent::TrailersOnly(
                    response_result,
                    response_context.metadata,
                ))
                .await;
            return;
        }

        // TODO: For unary response, batch this together with the
        let _ = response_sender
            .send(ServerStreamResponseEvent::Trailers(
                response_result,
                response_context.metadata.trailer_metadata,
            ))
            .await;

        // NOTE: This is an implicit uncorking of the response_sender here if it
        // was previously corked.
    }

    /// Creates a simple http response from an Error
    ///
    /// NOTE: When failures occur before the service is called, the server won't
    /// return any head or trailer metadata.
    ///
    /// TODO: Consider eventually supporting the passing of metadata to enable
    /// tracing of RPCs.
    fn error_response(error: Error, response_type: RPCMediaType) -> http::Response {
        // TODO: THis should dispatch it as a trailer only set of headers..

        let (mut sender, receiver) = spsc::bounded(1);
        sender
            .try_send(ServerStreamResponseEvent::Trailers(
                Err(error),
                Metadata::new(),
            ))
            .map_err(|e| e.error)
            .unwrap();

        // NOTE: GRPC servers are supported to always return 200 statuses.
        http::ResponseBuilder::new()
            .status(OK)
            .header(CONTENT_TYPE, response_type.to_string())
            .body(Box::new(ResponseBody {
                child_task: None,
                response_receiver: receiver,
                response_type,
                buffer: BufferQueue::new(),
                buffer_cursor: BufferQueueCursor::default(),
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
        let mut res = self.handle_request_impl(request, context).await;
        if self.enable_cors {
            http::cors::allow_all_requests(&mut res);
        }

        if !res.head.headers.has(CACHE_CONTROL) {
            res.head
                .headers
                .raw_headers
                .push(Header::new(CACHE_CONTROL.into(), "no-cache".into()));
        }

        res
    }
}

struct ResponseBody {
    child_task: Option<ChildTask>,

    response_receiver: spsc::Receiver<ServerStreamResponseEvent>,

    response_type: RPCMediaType,

    // /// TODO: Re-use this buffer across multiple messages
    // remaining_bytes: Bytes,
    buffer: BufferQueue,
    buffer_cursor: BufferQueueCursor,

    trailers: Option<Headers>,

    /// If true, then all body data remaining in the response is in the
    /// 'buffer' and 'trailers'.
    done_data: bool,
}

impl ResponseBody {
    fn process_event(&mut self, event: ServerStreamResponseEvent) -> Result<()> {
        if self.done_data {
            return Err(err_msg("Should not be getting events after data is done"));
        }

        match event {
            ServerStreamResponseEvent::Head(_) => {
                return Err(err_msg("Unexpected head event"));
            }
            ServerStreamResponseEvent::TrailersOnly(_, _) => {
                return Err(err_msg("Unexpected trailers-only event"));
            }
            ServerStreamResponseEvent::Message(data) => {
                // NOTE: This supports zero length packets are the message serializer will
                // always prepend a fixed length prefix.

                self.buffer
                    .push(MessageSerializer::serialize_header(&data, false));
                self.buffer.push(data);
            }
            ServerStreamResponseEvent::Trailers(result, trailer_meta) => {
                let mut trailers = Headers::new();
                trailer_meta.append_to_headers(&mut trailers)?;

                Self::append_result_to_headers(result, &mut trailers)?;

                match self.response_type.protocol {
                    RPCMediaProtocol::Default => {
                        self.trailers = Some(trailers);
                    }
                    RPCMediaProtocol::Web => {
                        let mut data = vec![];
                        trailers.serialize(&mut data)?;

                        // TODO: Given this is a known length, append it to the beginning of the
                        // 'data' vec.
                        self.buffer
                            .push(MessageSerializer::serialize_header(&data, true));
                        self.buffer.push(data.into());
                    }
                }

                self.done_data = true;
            }
        }

        Ok(())
    }

    fn append_result_to_headers(result: Result<()>, headers: &mut Headers) -> Result<()> {
        match result {
            Ok(()) => {
                Status::ok().append_to_headers(headers)?;
            }
            Err(error) => {
                // TODO: Have some default error handler to log the raw errors.
                // TODO: Only forward statuses that were generated locally and not ones
                // that were returned as part of an internal client RPC call.

                eprintln!("[rpc::Server] RPC Error: {}", error);
                let status = match error.downcast_ref::<Status>() {
                    Some(s) => s.clone(),
                    None => Status::internal("Internal error occured"),
                };

                status.append_to_headers(headers)?;
            }
        }

        Ok(())
    }
}

#[async_trait]
impl Readable for ResponseBody {
    async fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        loop {
            let n = self.buffer.read(&mut self.buffer_cursor, buf).unwrap();
            if n != 0 {
                self.buffer.advance(&self.buffer_cursor);

                // NOTE: We always stop after at least some amount of data is available to
                // ensure that readers are unblocked.
                return Ok(n);
            }

            if self.done_data {
                return Ok(0);
            }

            // TODO: Try to optimistically pull any already available messages from this.
            let event = self.response_receiver.recv().await?;

            self.process_event(event)?;
        }
    }
}

#[async_trait]
impl Body for ResponseBody {
    // We do want to hint:
    // - For unary responses or immediate error responses, this should be obvious.
    fn len(&self) -> Option<usize> {
        if self.done_data {
            return Some(self.buffer.len());
        }

        None
    }

    fn has_trailers(&self) -> bool {
        if self.done_data {
            return self.trailers.is_some();
        }

        // In web mode, we don't use trailers. Instead trailers are in the main data.
        self.response_type.protocol == RPCMediaProtocol::Default
    }

    async fn trailers(&mut self) -> Result<Option<Headers>> {
        if !(self.done_data && self.buffer.is_empty()) {
            return Err(err_msg("Trailers read at the wrong time"));
        }

        Ok(self.trailers.take())
    }
}
