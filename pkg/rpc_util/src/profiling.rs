use std::time::Duration;

use common::bytes::Bytes;
use common::errors::*;
use protobuf::Message;

pub trait AddProfilingEndpoints {
    fn add_profilez(&mut self) -> Result<()>;
}

impl AddProfilingEndpoints for rpc::Http2Server {
    fn add_profilez(&mut self) -> Result<()> {
        self.add_request_handler("/profilez", ProfilezRequestHandler {})
    }
}

struct ProfilezRequestHandler {}

impl ProfilezRequestHandler {
    async fn handle_impl(&self) -> Result<Bytes> {
        let profile = perf::profile_self(Duration::from_secs(10)).await?;
        let data = profile.serialize()?.into();

        Ok(data)
    }
}

// TODO: Just make this an http::RequestHandler
#[async_trait]
impl http::ServerHandler for ProfilezRequestHandler {
    async fn handle_request<'a>(
        &self,
        request: http::Request,
        _context: http::ServerRequestContext<'a>,
    ) -> http::Response {
        let data = match self.handle_impl().await {
            Ok(v) => v,
            Err(e) => {
                eprintln!("Error while running /profilez: {}", e);

                return http::ResponseBuilder::new()
                    .status(http::status_code::INTERNAL_SERVER_ERROR)
                    .body(http::BodyFromData(""))
                    .build()
                    .unwrap();
            }
        };

        http::ResponseBuilder::new()
            .status(http::status_code::OK)
            .body(http::BodyFromData(data))
            .header(http::header::CONTENT_TYPE, "application/octet-stream")
            .build()
            .unwrap()
    }
}
