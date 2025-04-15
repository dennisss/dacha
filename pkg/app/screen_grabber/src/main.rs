#[macro_use]
extern crate macros;
#[macro_use]
extern crate common;

use std::{collections::HashMap, sync::Arc, time::Instant};

use base_error::*;
use executor_multitask::RootResource;
use file::LocalPathBuf;
use google_auth::GoogleServiceAccount;
use parsing::ascii::AsciiString;
use rpc_util::NamedPortArg;
use screen_grabber::service::ScreenGrabberImpl;
use screen_grabber_proto::screen_grabber::ScreenGrabberIntoService;
use cluster_client::{ClusterServer, ClusterMetaClient};


const SERVICE_ACL_PROTO: &'static str = r#"
    rules: [
        {
            path: "/"
            is_directory: false
            principals: ["authenticated"]
        },
        {
            path: "/favicon.ico"
            is_directory: false
            principals: ["authenticated"]
        },
        {
            path: "/assets"
            is_directory: true
            principals: ["authenticated"]
        },
        {
            path: "/rpc/screen_grabber.ScreenGrabber"
            is_directory: true
            principals: ["authenticated"]
        }
    ]
"#;

#[derive(Args)]
struct Args {
    port: NamedPortArg,
}

async fn not_found_handle_request(mut req: http::Request) -> http::Response {
    http_util::not_found()
}

#[executor_main]
async fn main() -> Result<()> {
    let args = common::args::parse_args::<Args>()?;

    let service = RootResource::new();

    println!("Starting...");
    let start_time = Instant::now();

    let client = ClusterMetaClient::create_from_environment().await?;
    service.register_dependency(client.clone()).await;

    let mut acl = container_proto::cluster::ServiceACLProto::default();
    protobuf::text::parse_text_proto(SERVICE_ACL_PROTO, &mut acl)?;

    let mut server = ClusterServer::new(args.port.value(), acl, client)?;

    let data = file::read_to_string("/home/dennis/.credentials/da-cha-c2d195c05521.json").await?;

    let service_account: Arc<GoogleServiceAccount> =
        Arc::new(GoogleServiceAccount::parse_json(&data)?);

    let mut inst = Arc::new(ScreenGrabberImpl::create(service_account).await?);
    // service.register_dependency(inst.clone()).await;
    server.add_service(inst.clone().into_service())?;

    let web_handler = web::WebPageHandler::create(web::WebPageOptions {
        title: "Screen Grabber".into(),
        script_path: "built/pkg/app/screen_grabber/app.js".into(),
        vars: None,
    }).await?;
    server.add_request_handler("/", false, web_handler)?;
    server.add_request_handler("/assets", true, web::assets_handler())?;
    server.add_request_handler("/favicon.ico", false, http::HttpFn(not_found_handle_request))?;

    service.register_dependency(server.start()?).await;


    // TODO: Actually wait for resource readiness and make this a standard metric
    // that we report.
    let end_time = Instant::now();

    println!("Ready! Startup took {:?}", end_time - start_time);

    service.wait().await
}
