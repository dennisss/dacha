use base_error::*;
use http::status_code::MOVED_PERMANENTLY;
use http::server::ServerResource;
use http::header::{CONTENT_TYPE, LOCATION};
use parsing::ascii::AsciiString;
use net::ip::IPAddress;

pub fn start_bridge_http_server() -> ServerResource {
    let mut options = http::ServerOptions::default();
    options.ip = IPAddress::V4([127, 0, 0, 80]);
    options.port = Some(80);

    let handler = Service {};

    let server = http::Server::new(handler, options);
    server.start()
}

struct Service {}

#[async_trait]
impl http::ServerHandler for Service {
    // TODO: Block external connections

    async fn handle_request<'a>(
        &self,
        req: http::Request,
        ctx: http::ServerRequestContext<'a>,
    ) -> http::Response {
        let mut uri = req.head.uri.clone();
        uri.scheme = Some(AsciiString::new("https"));
        
        let uri_str = match uri.to_string() {
            Ok(v) => v,
            Err(e) => {
                return http_util::internal_server_error();
            }
        };

        http::ResponseBuilder::new()
            .status(MOVED_PERMANENTLY)
            .header(LOCATION, uri_str)
            .body(http::EmptyBody())
            .build()
            .unwrap()
    }
}
