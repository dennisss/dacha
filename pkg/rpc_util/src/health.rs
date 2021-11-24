use common::errors::*;

pub trait AddHealthEndpoints {
    fn add_healthz(&mut self) -> Result<()>;
}

impl AddHealthEndpoints for rpc::Http2Server {
    fn add_healthz(&mut self) -> Result<()> {
        self.add_request_handler("/healthz", HealthzRequestHandler {})
    }
}

struct HealthzRequestHandler {}

#[async_trait]
impl http::RequestHandler for HealthzRequestHandler {
    async fn handle_request(&self, request: http::Request) -> http::Response {
        http::ResponseBuilder::new()
            .status(http::status_code::OK)
            .body(http::BodyFromData("OK"))
            .header(http::header::CONTENT_TYPE, "text/plain")
            .build()
            .unwrap()
    }
}
