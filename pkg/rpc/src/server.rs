use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::marker::PhantomData;

use common::CancellationToken;
use common::bytes::Buf;
use common::errors::*;
use common::async_std::channel;
use common::task::ChildTask;
use common::io::Readable;
use common::bytes::Bytes;
use http::header::*;
use http::status_code::*;
use http::Body;

use crate::server_types::*;
use crate::metadata::Metadata;
use crate::service::Service;
use crate::constants::GRPC_PROTO_TYPE;
use crate::message::*;
use crate::status::*;

pub struct Http2Server {
    handler: Http2ResponseHandler,
    shutdown_token: Option<Box<dyn CancellationToken>>
}

impl Http2Server {
    pub fn new() -> Self {
        Self {
            handler: Http2ResponseHandler {
                services: HashMap::new(),
            },
            shutdown_token: None
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

    pub fn set_shutdown_token(&mut self, token: Box<dyn CancellationToken>) {
        self.shutdown_token = Some(token);
    }

    pub fn run(mut self, port: u16) -> impl Future<Output=Result<()>> + 'static {
        // TODO: Force usage of HTTP2.
        let mut server = http::Server::new(self.handler, http::ServerOptions::default());
        if let Some(token) = self.shutdown_token.take() {
            server.set_shutdown_token(token);
        }

        server.run(port)
    }
}

struct Http2ResponseHandler {
    services: HashMap<String, Arc<dyn Service>>,
}

impl Http2ResponseHandler {

    async fn handle_request_impl(&self, request: http::Request) -> Result<http::Response> {
        // TODO: Convert as many of the errors in this function as possible to gRPC
        // trailing status codes.
        
        // TODO: Should support different methods 
        if request.head.method != http::Method::POST {
            return http::ResponseBuilder::new()
                .status(http::status_code::METHOD_NOT_ALLOWED)
                .build();
        }

        let request_context = ServerRequestContext {
            metadata: Metadata::from_headers(&request.head.headers)?
        };

        let path_parts = request
            .head
            .uri
            .path
            .as_ref()
            .split('/')
            .map(|v| v.to_string())
            .collect::<Vec<_>>();
        if path_parts.len() != 3 || path_parts[0].len() != 0 {
            return Err(err_msg("Invalid path"));
        }

        let service = self
            .services
            .get(&path_parts[1])
            // TODO: Return an rpc::Status
            .ok_or(format_err!("Unknown service named: {}", path_parts[1]))?;

        let request = ServerStreamRequest {
            request_body: request.body,
            context: request_context,
            phantom_t: PhantomData,
        };

        let (response_sender, response_receiver) = channel::bounded(2);

        let method_name = path_parts[2].clone();

        let child_task = ChildTask::spawn(Self::service_caller(
            service.clone(), method_name, request, response_sender));

        let head_metadata = match response_receiver.recv().await? {
            ServerStreamResponseEvent::Head(metadata) => metadata,
            _ => { return Err(err_msg("Expected a head before other parts of the response")); }
        };

        let response_builder = http::ResponseBuilder::new()
            .status(OK)
            .header(CONTENT_TYPE, GRPC_PROTO_TYPE);

        let body = Box::new(ResponseBody {
            child_task,
            response_receiver,
            remaining_bytes: Bytes::new(),
            done_data: false,
            trailers: None,
        });

        let mut response = response_builder.body(body).build()?;
        head_metadata.append_to_headers(&mut response.head.headers)?;

        Ok(response)
    }

    // Wrapper which calls the Service method for a single request and ensures that a trailer is
    // eventually sent.
    async fn service_caller(service: Arc<dyn Service>, method_name: String,
        request: ServerStreamRequest<()>, response_sender: channel::Sender<ServerStreamResponseEvent>) {

        let mut response_context = ServerResponseContext::default();

        let mut head_sent = false;

        let response = ServerStreamResponse {
            phantom_t: PhantomData,
            context: &mut response_context,
            head_sent: &mut head_sent,
            sender: response_sender.clone(),
        };

        // TODO: If this fails with an error that can be downcast to a status, should we propagate
        // that back to the client.
        //
        // Probably no because this may imply that it was an internal RPC failure.
        // TODO: Ensure that similarly internal HTTP2 calls aren't propagated to clients.
        let response_result =  service.call(
            &method_name, request, response).await;

        if !head_sent {
            let _ = response_sender.send(ServerStreamResponseEvent::Head(
                response_context.metadata.head_metadata)).await;
        }

        let _ = response_sender.send(ServerStreamResponseEvent::Trailers(
            response_result, response_context.metadata.trailer_metadata)).await;
    }
}

#[async_trait]
impl http::RequestHandler for Http2ResponseHandler {
    async fn handle_request(&self, request: http::Request) -> http::Response {
        match self.handle_request_impl(request).await {
            Ok(r) => r,
            // TODO: Instead always use the trailers?
            // TODO: Don't share the raw error.
            Err(e) => http::ResponseBuilder::new()
                .status(INTERNAL_SERVER_ERROR)
                .header(CONTENT_TYPE, "text/plain")
                .body(http::BodyFromData(e.to_string().bytes().collect::<Vec<u8>>()))
                .build()
                .unwrap(),
        }
    }
}

/*
    Suppose I have a write API:
*/

struct ResponseBody {
    child_task: ChildTask,

    response_receiver: channel::Receiver<ServerStreamResponseEvent>,

    /// TODO: Re-use this buffer across multiple messages
    remaining_bytes: Bytes,

    /// If true, then we'll completely read all data.
    done_data: bool,

    trailers: Option<Headers>
}

#[async_trait]
impl Readable for ResponseBody {
    async fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        loop {
            if !self.remaining_bytes.is_empty() {
                let n = std::cmp::min(self.remaining_bytes.len(), buf.len());
                buf[0..n].copy_from_slice(&self.remaining_bytes[0..n]);

                self.remaining_bytes.advance(n);
                
                // NOTE: We always stop after at least some amount of data is available to ensure
                // that readers are unblocked.
                return Ok(n);
            }

            if self.done_data {
                return Ok(0);
            }

            let event = self.response_receiver.recv().await?;
            match event {
                ServerStreamResponseEvent::Head(_) => {
                    return Err(err_msg("Unexpected head event"));
                },
                ServerStreamResponseEvent::Message(data) => {
                    // NOTE: This supports zero length packets are the message serializer will
                    // always prepend a fixed length prefix.
                    self.remaining_bytes = Bytes::from(MessageSerializer::serialize(&data));
                },
                ServerStreamResponseEvent::Trailers(result, trailer_meta) => {

                    let mut trailers = Headers::new();
                    trailer_meta.append_to_headers(&mut trailers)?;
           
                    match result {
                        Ok(()) => {
                            Status::ok().append_to_headers(&mut trailers)?;
                        }
                        Err(error) => {
                            // TODO: Have some default error handler to log the raw errors.
                            
                            eprintln!("RPC Error: {:?}", error);
                            let status = match error.downcast_ref::<Status>() {
                                Some(s) => s.clone(),
                                None => {
                                    Status {
                                        code: crate::StatusCode::Internal,
                                        message: "Internal error occured".into()
                                    }
                                }
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