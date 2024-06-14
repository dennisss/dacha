#[macro_use]
extern crate common;
#[macro_use]
extern crate macros;

use std::{sync::Arc, time::Duration};

use common::{errors::*, io::Readable};
use executor::bundle::TaskResultBundle;
use executor_multitask::RootResource;
use rpc_test::proto::adder::AdderIntoService;

struct NullBody {}

#[async_trait]
impl http::Body for NullBody {
    fn len(&self) -> Option<usize> {
        None
    }

    fn has_trailers(&self) -> bool {
        false
    }

    async fn trailers(&mut self) -> Result<Option<http::Headers>> {
        Ok(None)
    }
}

#[async_trait]
impl Readable for NullBody {
    async fn read(&mut self, out: &mut [u8]) -> Result<usize> {
        executor::sleep(Duration::from_secs(1)).await?;
        for i in 0..out.len() {
            out[i] = 0;
        }

        Ok(out.len())
    }
}

async fn null_handler(request: http::Request) -> http::Response {
    http::ResponseBuilder::new()
        .status(http::status_code::OK)
        .body(Box::new(NullBody {}))
        .build()
        .unwrap()
}

#[executor_main]
async fn main() -> Result<()> {
    let service = RootResource::new();

    service
        .register_dependency({
            let web_handler = web::WebServerHandler::new(web::WebServerOptions {
                pages: vec![web::WebPageOptions {
                    title: "Adder".into(),
                    path: "/".into(),
                    script_path: "built/pkg/web/app.js".into(),
                    vars: None,
                }],
            });

            let mut options = http::ServerOptions::default();
            options.name = "WebServer".to_string();
            options.port = Some(8000);

            let web_server = http::Server::new(web_handler, options);

            Arc::new(web_server.start())
        })
        .await;

    service
        .register_dependency({
            let mut rpc_server = rpc::Http2Server::new(Some(8001));
            rpc_server.add_service(rpc_test::AdderImpl::create(None).await?.into_service())?;
            rpc_server.add_request_handler("/null", http::HttpFn(null_handler));
            rpc_server.enable_cors();
            rpc_server.allow_http1();
            rpc_server.start()
        })
        .await;

    service.wait().await
}
