extern crate common;
extern crate http;
extern crate rpc;
extern crate rpc_test;
extern crate web;
#[macro_use]
extern crate macros;

use common::errors::*;
use executor::bundle::TaskResultBundle;
use rpc_test::proto::adder::AdderIntoService;

#[executor_main]
async fn main() -> Result<()> {
    let mut task_bundle = TaskResultBundle::new();

    task_bundle.add("WebServer", {
        let web_handler = web::WebServerHandler::new(web::WebServerOptions {
            pages: vec![web::WebPageOptions {
                title: "Adder".into(),
                path: "/".into(),
                script_path: "built/pkg/web/app.js".into(),
                vars: None,
            }],
        });

        let web_server = http::Server::new(web_handler, http::ServerOptions::default());

        web_server.run(8000)
    });

    task_bundle.add("RpcServer", {
        let mut rpc_server = rpc::Http2Server::new();
        rpc_server.add_service(rpc_test::AdderImpl::create(None).await?.into_service())?;
        rpc_server.enable_cors();
        rpc_server.allow_http1();
        rpc_server.run(8001)
    });

    task_bundle.join().await?;

    Ok(())
}
