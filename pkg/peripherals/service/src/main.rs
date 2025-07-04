#[macro_use]
extern crate macros;
#[macro_use]
extern crate common;

use std::{collections::HashMap, sync::Arc, time::Instant};

use base_error::*;
use executor_multitask::RootResource;
use file::LocalPathBuf;
use peripherals_service::service::PeripheralsImpl;
use peripherals_proto::peripherals::PeripheralsIntoService;
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
            path: "/rpc/peripherals.Peripherals"
            is_directory: true
            principals: ["group:cluster-owners"]
        }
    ]
"#;

#[derive(Args)]
struct Args {
    port: NamedPortArg,
    config_name: String,
}

#[executor_main]
async fn main() -> Result<()> {
    let args = common::args::parse_args::<Args>()?;

    let service = RootResource::new();

    let start_time = Instant::now();

    let mut configs = peripherals_service::config::load_board_configs().await?;
    let config = configs.remove(&args.config_name)
        .ok_or_else(|| err_msg("No config with the given name"))?;

    let client = ClusterMetaClient::create_from_environment().await?;
    service.register_dependency(client.clone()).await;

    let mut acl = container_proto::cluster::ServiceACLProto::default();
    protobuf::text::parse_text_proto(SERVICE_ACL_PROTO, &mut acl)?;

    let mut server = ClusterServer::new(args.port.value(), acl, client)?;

    let mut inst = Arc::new(PeripheralsImpl::create(config).await?);
    service.register_dependency(inst.clone()).await;
    server.add_service(inst.clone().into_service())?;

    let web_handler = web::WebPageHandler::create(web::WebPageOptions {
        title: "Peripherals UI".into(),
        script_path: "built/pkg/peripherals/service/app.js".into(),
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
