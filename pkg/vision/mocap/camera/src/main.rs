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
use ptp_proto::ptp::*;
use cluster_client::{ClusterServer, ClusterMetaClient};
use mocap_proto::mocap::MocapCameraIntoService;
use mocap_camera::MocapCamera;


/*

# Local Testing

cargo run --bin mocap_camera -- \
    --rpc_port=8001 \
    --ptp_port=8002 \
    --ptp_iface=eth0 \
    --dummy

cargo run --bin builder -- build //pkg/vision/mocap/camera:app

# Remote Testing

cargo run --bin cluster_cli -- start_job pkg/net/ptp/config/ptp_follower.job

cargo run --bin cluster_cli -- list workers

cargo run --bin cluster_cli -- log --worker_name=ptp_follower.amv5hvhdjxktg --latest_attempt

*/



const SERVICE_ACL_PROTO: &'static str = r#"
    rules: [
        {
            path: "/"
            is_directory: false
            principals: ["authenticated"]
        },
        {
            path: "/rpc/ptp.TimeSync"
            is_directory: true
            principals: ["authenticated"]
        },
        {
            path: "/rpc/mocap.MocapCamera"
            is_directory: true
            principals: ["authenticated"]
        },
        {
            path: "/camera"
            is_directory: false
            principals: ["authenticated"]
        }
    ]
"#;

pub struct CameraHttpHandler {
    inst: Arc<MocapCamera>,
}

#[async_trait]
impl http::ServerHandler for CameraHttpHandler {
    async fn handle_request<'a>(
        &self,
        req: http::Request,
        ctx: http::ServerRequestContext<'a>,
    ) -> http::Response {
        self.inst.live_stream()
    }
}



#[derive(Args)]
struct Args {
    rpc_port: NamedPortArg,

    ptp_port: NamedPortArg,

    ptp_iface: String,

    #[arg(default = false)]
    dummy: bool,
}

#[executor_main]
async fn main() -> Result<()> {

    let args = common::args::parse_args::<Args>()?;

    let service = RootResource::new();

    let client = ClusterMetaClient::create_from_environment().await?;
    service.register_dependency(client.clone()).await;

    let mut acl = cluster_proto::cluster::ServiceACLProto::default();
    protobuf::text::parse_text_proto(SERVICE_ACL_PROTO, &mut acl)?;

    let mut server = ClusterServer::new(args.rpc_port.value(), acl, client.clone())?;

    let web_handler = web::WebPageHandler::create(web::WebPageOptions {
        title: "Mocap Camera".into(),
        script_path: "built/pkg/vision/mocap/camera/app.js".into(),
        vars: None,
    }).await?;
    server.add_request_handler("/", false, web_handler)?;

    println!("RPC Port: {}", args.rpc_port.value());
    println!("PTP Port: {}", args.ptp_port.value());

    if args.dummy {
        let cam = Arc::new(mocap_camera::DummyMocapCamera::create(1).await?);
        server.add_service(cam.clone().into_service())?;

        let time_sync = Arc::new(ptp::DummyTimeSyncNode::create());
        server.add_service(time_sync.clone().into_service())?;

    } else {
        // TODO: Need to ensure that the device matches the interface.
        let mut ptp_device = Arc::new(ptp::PTPDevice::open_default()?);
        ptp_device.configure_pps_output()
            .map_err(|e| format_err!("While enabling PPS output: {}", e))?;

        let ptp_socket = ptp::TimestampedUdpSocket::create(
            format!("0.0.0.0:{}", args.ptp_port.value()).parse()?,
            &args.ptp_iface
        ).await?;

        let time_sync = Arc::new(ptp::TimeSyncNode::create(client.clone(), ptp_device.clone(), ptp_socket).await);

        service.register_dependency(time_sync.clone()).await;
        server.add_service(time_sync.clone().into_service())?;

        let cam = Arc::new(MocapCamera::create(ptp_device.clone()).await?);
        service.register_dependency(cam.clone()).await;
        server.add_service(cam.clone().into_service())?;
        
        server.add_request_handler("/camera", true, CameraHttpHandler { inst: cam.clone() })?;
    }


    service.register_dependency(server.start()?).await;


    println!("Running...");
    service.wait().await
}