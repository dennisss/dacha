use std::collections::HashMap;
use std::sync::Arc;

use common::errors::*;
use http::header::*;
use http::status_code::*;

use crate::request::*;
use crate::response::*;
use crate::metadata::Metadata;
use crate::service::Service;
use crate::constants::GRPC_PROTO_TYPE;
use crate::message::*;
use crate::status::*;

pub struct Http2Server {
    port: u16,
    services: HashMap<String, Arc<dyn Service>>,
}

impl Http2Server {
    pub fn new(port: u16) -> Self {
        Self {
            port,
            services: HashMap::new(),
        }
    }

    pub fn add_service(&mut self, service: Arc<dyn Service>) -> Result<()> {
        let service_name = service.service_name().to_string();
        if self.services.contains_key(&service_name) {
            return Err(err_msg("Adding duplicate service to RPCServer"));
        }

        self.services.insert(service_name, service);
        Ok(())
    }

    pub async fn run(self) -> Result<()> {
        let server = http::Server::new(self.port, self);
        server.run().await
    }

    async fn handle_request_impl(&self, mut request: http::Request) -> Result<http::Response> {
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
            .ok_or(format_err!("Unknown service named: {}", path_parts[1]))?;

        let mut reader = MessageReader::new(request.body.as_mut());

        let request_bytes = reader.read().await?
            .ok_or_else(|| err_msg("No request body received"))?;
        // TODO: Assert no more data in the body.

        // TODO: If this fails with an error that can be downcast to a status, should we propagate
        // that back to the client.
        //
        // Probably no because this may imply that it was an internal RPC failure.
        // TODO: Ensure that similarly internal HTTP2 calls aren't propagated to clients.
        let (response_context, response_result) =  service.call(
            &path_parts[2], request_context, request_bytes).await?;
        
        let response_builder = http::ResponseBuilder::new()
            .status(OK)
            .header(CONTENT_TYPE, GRPC_PROTO_TYPE);

        let mut trailers = Headers::new();
        response_context.metadata.trailer_metadata.append_to_headers(&mut trailers)?;

        let body = match response_result {
            Ok(data) => {
                Status::ok().append_to_headers(&mut trailers)?;
                http::WithTrailers(UnaryMessageBody::new(data), trailers)
            }
            Err(status) => {
                status.append_to_headers(&mut trailers);
                http::WithTrailers(http::EmptyBody(), trailers)
            }
        };
 
        let mut response = response_builder.body(body).build()?;
        response_context.metadata.head_metadata.append_to_headers(&mut response.head.headers)?;
        Ok(response)
    }
}

#[async_trait]
impl http::RequestHandler for Http2Server {
    async fn handle_request(&self, request: http::Request) -> http::Response {
        match self.handle_request_impl(request).await {
            Ok(r) => r,
            // TODO: Instead always use the trailers?
            Err(e) => http::ResponseBuilder::new()
                .status(INTERNAL_SERVER_ERROR)
                .header(CONTENT_TYPE, "text/plain")
                .body(http::BodyFromData(e.to_string().bytes().collect::<Vec<u8>>()))
                .build()
                .unwrap(),
        }
    }
}
