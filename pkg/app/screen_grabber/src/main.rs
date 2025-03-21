#[macro_use]
extern crate macros;
#[macro_use]
extern crate common;

use std::{collections::HashMap, sync::Arc, time::Instant};

use base_error::*;
use executor_multitask::RootResource;
use file::LocalPathBuf;
use google_auth::GoogleServiceAccount;
use http::static_file_handler::StaticFileHandlerOptions;
use http::{
    static_file_handler::{StaticFileBody, StaticFileHandler},
    ServerHandler,
};
use parsing::ascii::AsciiString;
use rpc_util::NamedPortArg;
use screen_grabber::service::ScreenGrabberImpl;
use screen_grabber_proto::screen_grabber::ScreenGrabberIntoService;
use web::WebServerHandler;

pub fn bad_request() -> http::Response {
    http::ResponseBuilder::new()
        .status(http::status_code::BAD_REQUEST)
        .build()
        .unwrap()
}

pub fn not_found_request() -> http::Response {
    http::ResponseBuilder::new()
        .status(http::status_code::NOT_FOUND)
        .build()
        .unwrap()
}

#[derive(Args)]
struct Args {
    port: NamedPortArg,
    tls_certificate: LocalPathBuf,
    tls_key: LocalPathBuf,
}

struct HttpHandler {
    instance: Arc<ScreenGrabberImpl>,
    inner: WebServerHandler,
    rpc_handler: rpc::Http2RequestHandler,
}

impl HttpHandler {
    async fn handle_request_impl<'a>(
        &self,
        mut request: http::Request,
        context: http::ServerRequestContext<'a>,
    ) -> http::Response {
        if let Some(path) = request.head.uri.path.as_str().strip_prefix("/rpc/") {
            request.head.uri.path = AsciiString::new(&format!("/{}", path));
            return self.rpc_handler.handle_request(request, context).await;
        }

        if request.head.uri.path.as_str().starts_with("/ui/") {
            request.head.uri.path = AsciiString::new("/");
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

    // TODO: Passthrough connection handling to the rpc hnadler.
}

#[executor_main]
async fn main() -> Result<()> {
    let args = common::args::parse_args::<Args>()?;

    let service = RootResource::new();

    println!("Starting...");
    let start_time = Instant::now();

    let certificate_file = file::read(args.tls_certificate).await?.into();
    let private_key_file = file::read(args.tls_key).await?.into();

    let mut tls_options =
        crypto::tls::ServerOptions::recommended(certificate_file, private_key_file)?;

    let data = file::read_to_string("/home/dennis/.credentials/da-cha-c2d195c05521.json").await?;

    let service_account: Arc<GoogleServiceAccount> =
        Arc::new(GoogleServiceAccount::parse_json(&data)?);

    let mut inst = Arc::new(ScreenGrabberImpl::create(service_account).await?);
    // service.register_dependency(inst.clone()).await;

    let mut rpc_handler = rpc::Http2RequestHandler::new();
    rpc_handler.add_service(inst.clone().into_service())?;

    service
        .register_dependency({
            let vars = json::Value::Object(map!(
                "rpc_port" => &json::Value::Number(args.port.value() as f64)
            ));

            let web_handler = web::WebServerHandler::new(web::WebServerOptions {
                pages: vec![web::WebPageOptions {
                    title: "Screen Grabber".into(),
                    path: "/".into(),
                    script_path: "built/pkg/app/screen_grabber/app.js".into(),
                    vars: Some(vars),
                }],
            });

            let handler = HttpHandler {
                instance: inst,
                inner: web_handler,
                rpc_handler,
            };

            let mut options = http::ServerOptions::default();
            options.port = Some(args.port.value());
            options.tls = Some(tls_options.into());
            options.force_http2 = true;

            let web_server = http::Server::new(handler, options);
            Arc::new(web_server.start())
        })
        .await;

    // TODO: Actually wait for resource readiness and make this a standard metric
    // that we report.
    let end_time = Instant::now();

    println!("Ready! Startup took {:?}", end_time - start_time);

    service.wait().await
}
