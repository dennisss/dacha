use std::collections::HashMap;
use std::sync::Arc;

use common::errors::*;
use http::header::*;
use http::status_code::*;
use protobuf::service::{Channel, Service};

use crate::constants::GRPC_PROTO_TYPE;

pub struct Server {
    port: u16,
    services: HashMap<String, Arc<dyn Service>>,
}

impl Server {
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
            .header(CONTENT_TYPE, GRPC_PROTO_TYPE)
            .body(http::BodyFromData(response_bytes))
            .build()
    }
}

#[async_trait]
impl http::RequestHandler for Server {
    async fn handle_request(&self, request: http::Request) -> http::Response {
        match self.handle_request_impl(request).await {
            Ok(r) => r,
            // TODO: Instead always use the trailers?
            Err(e) => http::ResponseBuilder::new()
                .status(OK)
                .header(CONTENT_TYPE, "text/plain")
                .body(http::BodyFromData(e.to_string().bytes().collect::<Vec<u8>>()))
                .build()
                .unwrap(),
        }
    }
}
