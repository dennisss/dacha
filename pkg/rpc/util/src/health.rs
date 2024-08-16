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

// TODO: Just make this an http::RequestHandler
#[async_trait]
impl http::ServerHandler for HealthzRequestHandler {
    async fn handle_request<'a>(
        &self,
        request: http::Request,
        _context: http::ServerRequestContext<'a>,
    ) -> http::Response {
        http::ResponseBuilder::new()
            .status(http::status_code::OK)
            .body(http::BodyFromData("OK"))
            .header(http::header::CONTENT_TYPE, "text/plain")
            .build()
            .unwrap()
    }
}
