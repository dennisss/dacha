#[macro_use]
extern crate common;
extern crate http;
extern crate protobuf;

#[macro_use]
extern crate macros;

pub mod proto;

use common::bytes::Bytes;
use common::errors::*;
use http::header::*;
use http::status_code::*;
use protobuf::service::{Channel, Service};
use std::collections::HashMap;
use std::sync::Arc;

const GRPC_PROTO_TYPE: &'static str = "application/grpc+proto";

pub struct RPCServer {
    port: u16,
    services: HashMap<String, Arc<dyn Service>>,
}

impl RPCServer {
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

        let mut request_bytes = vec![];
        request.body.read_to_end(&mut request_bytes).await?;

        let response_bytes = service.call(&path_parts[2], request_bytes.into()).await?;

        http::ResponseBuilder::new()
            .status(OK)
            .header(CONTENT_TYPE, "application/grpc+proto")
            .body(http::BodyFromData(response_bytes))
            .build()
    }
}

#[async_trait]
impl http::RequestHandler for RPCServer {
    async fn handle_request(&self, request: http::Request) -> http::Response {
        println!("GOT REQUEST {:#?}", request.head);

        match self.handle_request_impl(request).await {
            Ok(r) => r,
            Err(e) => http::ResponseBuilder::new()
                .status(OK)
                .header(CONTENT_TYPE, "text/plain")
                .body(http::BodyFromData(e.to_string().bytes().collect::<Vec<u8>>()))
                .build()
                .unwrap(),
        }
    }
}

pub struct RPCChannel {
    client: http::Client,
}

impl RPCChannel {
    pub fn create(uri: &str) -> Result<Self> {
        Ok(Self {
            client: http::Client::create(uri)?,
        })
    }
}

#[async_trait]
impl Channel for RPCChannel {
    async fn call(
        &self,
        service_name: &'static str,
        method_name: &'static str,
        request_bytes: Bytes,
    ) -> Result<Bytes> {
        let request = http::RequestBuilder::new()
            .method(http::Method::POST)
            .path(format!("/{}/{}", service_name, method_name))
            // TODO: No gurantee that we were given proto data.
            .header(CONTENT_TYPE, GRPC_PROTO_TYPE)
            .body(http::BodyFromData(request_bytes))
            .build()?;

        let mut response = self.client.request(request).await?;
        if response.head.status_code != OK {
            return Err(err_msg("Request failed"));
        }

        // TODO: Check Content-Type?

        let mut response_bytes = vec![];
        response.body.read_to_end(&mut response_bytes).await?;

        Ok(response_bytes.into())
    }
}
