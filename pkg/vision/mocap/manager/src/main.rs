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
use file::{project_path, LocalPathBuf};
use mocap_manager::*;
use http::static_file_handler::*;
use http_util::bad_request;
use cluster_client::id::entity_id_from_string;

// TODO: Automatically turn off the strobe on the cameras if there is no client for a while

// TODO: Eliminate the package dependency of mocap_manager on mocap_camera

/*

cargo run --bin mocap_manager --release -- --port=8000

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
            path: "/data"
            is_directory: true
            principals: ["authenticated"]
        },
        {
            path: "/camera"
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


pub struct CameraHttpHandler {
    inst: Arc<MocapManager>,
}

#[async_trait]
impl http::ServerHandler for CameraHttpHandler {
    async fn handle_request<'a>(
        &self,
        req: http::Request,
        ctx: http::ServerRequestContext<'a>,
    ) -> http::Response {
        let mut query = match http_util::parse_query(&req) {
            Ok(v) => v,
            Err(e) => return bad_request()
        };

        let camera_id_str = match query.remove("id") {
            Some(v) => v,
            None => return bad_request()
        };

        let camera_id = match entity_id_from_string(&camera_id_str) {
            Some(v) => v,
            None => return bad_request()
        };

        self.inst.live_stream(camera_id).await
    }
}


#[derive(Args)]
struct Args {
    port: NamedPortArg,
    data_dir: LocalPathBuf,
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

    let data_handler = StaticFileHandler::new_with_options(
        &args.data_dir,
        StaticFileHandlerOptions {
            trust_file_extension: true,
            mount_path: "/data".to_string(),
        },
    );
    server.add_request_handler("/data", true, data_handler)?;


    let manager = Arc::new(MocapManager::create(
        config,
        args.data_dir,
        // meta_client.clone()
    ).await?);
    service.register_dependency(manager.clone()).await;
    server.add_service(manager.to_service())?;

    server.add_request_handler("/camera", true, CameraHttpHandler { inst: manager.clone() })?;

    service.register_dependency(server.start()?).await;

    println!("Server running on port {}...", args.port.value());

    service.wait().await
}

