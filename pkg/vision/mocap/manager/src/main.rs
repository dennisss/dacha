#[macro_use]
extern crate common;
#[macro_use]
extern crate macros;

use std::time::Duration;
use std::sync::Arc;

use common::args::list::CommaSeparated;
use common::errors::*;
use executor_multitask::RootResource;
use rpc_util::NamedPortArg;
use cluster_client::{ClusterServer, ClusterMetaClient};
use mocap_proto::mocap::*;
use cluster_client::service::create_rpc_channel;
use file::project_path;
use mocap_manager::*;

// TODO: Automatically turn off the strobe on the cameras if there is no client for a while

// TODO: Eliminate the package dependency of mocap_manager on mocap_camera

/*

cargo run --bin mocap_manager -- --port=8000

cargo run --bin builder -- build //pkg/vision/mocap/manager:app

cargo run --bin cluster_cli -- start_job pkg/vision/mocap/config/camera.job

cargo run --bin cluster_cli -- list workers

*/

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
            path: "/rpc/mocap.MocapManager"
            is_directory: true
            principals: ["authenticated"]
        }
    ]
"#;


#[derive(Args)]
struct Args {
    port: NamedPortArg
}

#[executor_main]
async fn main() -> Result<()> {

    let args = common::args::parse_args::<Args>()?;

    let mut config = MocapManagerConfig::default();
    protobuf::text::parse_text_proto(
        &file::read_to_string(project_path!("pkg/vision/mocap/config/manager.txtpb")).await?,
        &mut config
    )?;

    let service = RootResource::new();

    let meta_client = ClusterMetaClient::create_from_environment().await?;
    service.register_dependency(meta_client.clone()).await;

    let mut acl = cluster_proto::cluster::ServiceACLProto::default();
    protobuf::text::parse_text_proto(SERVICE_ACL_PROTO, &mut acl)?;

    let mut server = ClusterServer::new(args.port.value(), acl, meta_client.clone())?;

    let web_handler = Arc::new(web::WebPageHandler::create(web::WebPageOptions {
        title: "Mocap Manager".into(),
        script_path: "built/pkg/vision/mocap/manager/app.js".into(),
        vars: None,
    }).await?);
    server.add_request_handler("/", false, web_handler.clone())?;
    server.add_request_handler("/ui", true, web_handler.clone())?;


    let manager = Arc::new(MocapManager::create(
        config,
        meta_client.clone()
    ).await?);
    service.register_dependency(manager.clone()).await;
    server.add_service(manager.clone().into_service())?;

    service.register_dependency(server.start()?).await;

    service.wait().await
}

