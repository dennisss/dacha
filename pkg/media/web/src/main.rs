/*

cargo run --bin builder -- build //pkg/media/web:app

cargo run --bin media_web -- --web_port=8000

*/

#[macro_use]
extern crate common;
#[macro_use]
extern crate macros;

use std::sync::Arc;

use common::{errors::*, io::Readable};
use http::ServerHandler;
use media_web::camera_manager::CameraManager;
use rpc_util::NamedPortArg;
use web::WebServerHandler;

#[derive(Args)]
struct Args {
    /// Port on which to start the web server.
    web_port: NamedPortArg,
}

struct HttpHandler {
    usb_context: usb::Context,
    camera_manager: CameraManager,
    inner: WebServerHandler,
}

impl HttpHandler {
    async fn handle_request_impl<'a>(
        &self,
        request: http::Request,
        context: http::ServerRequestContext<'a>,
    ) -> http::Response {
        if request.head.uri.path.as_str() == "/camera" {
            return media_web::camera_stream::respond_with_any_camera_stream(
                &self.usb_context,
                &self.camera_manager,
                request,
            )
            .await;
        }

        self.inner.handle_request(request, context).await
    }
}

#[async_trait]
impl http::ServerHandler for HttpHandler {
    async fn handle_request<'a>(
        &self,
        request: http::Request,
        context: http::ServerRequestContext<'a>,
    ) -> http::Response {
        self.handle_request_impl(request, context).await
    }
}

#[executor_main]
async fn main() -> Result<()> {
    let args = common::args::parse_args::<Args>()?;

    let root_resource = executor_multitask::RootResource::new();

    root_resource
        .register_dependency({
            let vars = json::Value::Object(map!(
                "rpc_port" => &json::Value::Number(1000.0)
            ));

            let web_handler = web::WebServerHandler::new(web::WebServerOptions {
                pages: vec![web::WebPageOptions {
                    title: "Media Player".into(),
                    path: "/".into(),
                    script_path: "built/pkg/media/web/app.js".into(),
                    vars: Some(vars),
                }],
            });

            let camera_manager = CameraManager::default();
            let usb_context = usb::Context::create()?;

            let handler = HttpHandler {
                camera_manager,
                usb_context,
                inner: web_handler,
            };

            let mut options = http::ServerOptions::default();
            options.name = "WebServer".to_string();
            options.port = Some(args.web_port.value());

            let web_server = http::Server::new(handler, options);
            Arc::new(web_server.start())
        })
        .await;

    root_resource.wait().await
}
