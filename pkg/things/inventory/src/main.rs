#[macro_use]
extern crate macros;
#[macro_use]
extern crate common;

use std::{collections::HashMap, sync::Arc, time::Instant};

use base_error::*;
use executor_multitask::RootResource;
use inventory::service::InventoryImpl;
use inventory_proto::inventory::InventoryIntoService;
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
            path: "/ui"
            is_directory: true
            principals: ["authenticated"]
        },
        {
            path: "/rpc/inventory.Inventory"
            is_directory: true
            principals: ["group:cluster-owners"]
        }
    ]
"#;

/*
TODO: Can I make ids base64 strings directly.
*/

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

    let mut server = ClusterServer::new(args.port.value(), acl, client.clone())?;

    let mut inst = Arc::new(InventoryImpl::create(client.clone()).await?);
    // TODO: Need some warnings if we ever forget to register these.
    // service.register_dependency(inst.clone()).await;
    server.add_service(inst.clone().into_service())?;

    let web_handler = Arc::new(web::WebPageHandler::create(web::WebPageOptions {
        title: "Inventory".into(),
        script_path: "built/pkg/things/inventory/app.js".into(),
        vars: None,
    }).await?);
    server.add_request_handler("/", false, web_handler.clone())?;
    server.add_request_handler("/ui", true, web_handler.clone())?;


    service.register_dependency(server.start()?).await;

    // TODO: Actually wait for resource readiness and make this a standard metric
    // that we report.
    let end_time = Instant::now();

    println!("Ready! Startup took {:?}", end_time - start_time);

    service.wait().await
}
