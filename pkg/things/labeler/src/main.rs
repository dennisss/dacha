#[macro_use]
extern crate macros;
#[macro_use]
extern crate common;

/*
TODOs:
- 'g' renders below the print area with the default line spacing
*/

use std::{collections::HashMap, sync::Arc, time::Instant};

use base_error::*;
use executor_multitask::RootResource;
use file::LocalPathBuf;
use http::{
    static_file_handler::{StaticFileBody, StaticFileHandler},
    ServerHandler,
};
use labeler::service::LabelerImpl;
use labeler_proto::labeler::LabelerIntoService;
use parsing::ascii::AsciiString;
use rpc_util::NamedPortArg;
use cluster_client::{ClusterServer, ClusterMetaClient};

const SERVICE_ACL_PROTO: &'static str = r#"
    rules: [
        {
            path: "/"
            is_directory: false
            principals: ["authenticated"]
        },
        {
            path: "/rpc/labeler.Labeler"
            is_directory: true
            principals: ["group:cluster-owners"]
        }
    ]
"#;

#[derive(Args)]
struct Args {
    port: NamedPortArg,
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

    let mut inst = Arc::new(LabelerImpl::create().await?);
    // TODO: Need some warnings if we ever forget to register these.
    service.register_dependency(inst.clone()).await;
    server.add_service(inst.clone().into_service())?;

    let web_handler = web::WebPageHandler::create(web::WebPageOptions {
        title: "Labeler".into(),
        script_path: "built/pkg/things/labeler/app.js".into(),
        vars: None,
    }).await?;
    server.add_request_handler("/", false, web_handler)?;

    service.register_dependency(server.start()?).await;

    // TODO: Actually wait for resource readiness and make this a standard metric
    // that we report.
    let end_time = Instant::now();

    println!("Ready! Startup took {:?}", end_time - start_time);

    service.wait().await
}
