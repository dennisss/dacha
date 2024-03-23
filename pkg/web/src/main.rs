extern crate common;
extern crate http;
extern crate rpc;
extern crate rpc_test;
extern crate web;
#[macro_use]
extern crate macros;

use std::sync::Arc;

use common::errors::*;
use executor::bundle::TaskResultBundle;
use executor_multitask::RootResource;
use rpc_test::proto::adder::AdderIntoService;

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
            rpc_server.enable_cors();
            rpc_server.allow_http1();
            rpc_server.start()
        })
        .await;

    service.wait().await
}
