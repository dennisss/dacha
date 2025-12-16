/*
cargo run --bin cnc_controller -- service \
    --config_name=breadboard_motor \
    --port=8000

cargo run --bin cnc_controller -- service \
    --config_name=voron0 \
    --port=8000

cargo run --bin cnc_controller -- execute "move_to { x: 160 }"


cargo run --bin cnc_controller -- execute --proto="
    move_to { x: 50 } move_to { x: 50 y: 50 } move_to { x: 0 y: 50 } move_to { x: 0 y: 0 }
    move_to { x: 40 y: 0 }
    move_to { x: 90 y: 50 }
    move_to { x: 40 y: 50 }
    move_to { x: 90 y: 0 }
    move_to { x: 40 y: 0 }
    move_to { x: 0 y: 0 }
"

cargo run --bin cnc_controller -- execute "
    move_to { x: 0 y: 0 }
"

cargo run --bin cnc_controller -- execute "
    move_to { x: 0 y: 0 z: -10 }
    move_to { x: 0 y: 0 z: 10 }
    move_to { x: 0 y: 0 z: 0 }
"


cargo run --bin cnc_controller -- execute --gcode_file=testdata/cnc/voron0/voron0-calibration-cube.gcode

*/

use std::sync::Arc;
use std::time::Duration;

use common::errors::*;
use math::matrix::VectorXf;
use math::vecxf;
use executor_multitask::RootResource;
use cluster_client::ClusterMetaClient;
use cluster_client::ClusterServer;
use cnc_controller_proto::cnc::*;
use rpc_util::NamedPortArg;
use file::LocalPathBuf;
use cnc::linear_motion_planner::LinearMotionPlanner;

use crate::devices::*;
use crate::config::*;
use crate::motion_controller::*;
use crate::endstop_controller::*;
use crate::machine_controller::MachineController;

const SERVICE_ACL_PROTO: &'static str = r#"
    rules: [
        #{
        #    path: "/"
        #    is_directory: false
        #    principals: ["authenticated"]
        #},
        {
            path: "/rpc/cnc.Controller"
            is_directory: true
            principals: ["authenticated"]
        }
    ]
"#;


struct ControllerServiceImpl {
    machine: MachineController,
}

impl ControllerServiceImpl {
    async fn create(config: ControllerConfig) -> Result<Self> {
let machine = MachineController::create(config).await?;
        Ok(Self { machine })
    }
}

#[async_trait]
impl ControllerService for ControllerServiceImpl {
    async fn Execute(
        &self,
        request: rpc::ServerRequest<ExecuteRequest>,
        response: &mut rpc::ServerResponse<ExecuteResponse>,
    ) -> Result<()> {
        response.value = self.machine.execute(&request).await?;
        Ok(())
    }

    async fn GetPosition(
        &self,
        request: rpc::ServerRequest<GetPositionRequest>,
        response: &mut rpc::ServerResponse<GetPositionResponse>,
    ) -> Result<()> {
response.value = self.machine.get_position().await?;
        Ok(())
    }
}


#[derive(Args)]
pub struct ControllerServiceCommand {
    config_name: String,
    port: NamedPortArg,
}

impl ControllerServiceCommand {

    pub async fn run(self) -> Result<()> {        
        let service = RootResource::new();
        let client = ClusterMetaClient::create_from_environment().await?;
        service.register_dependency(client.clone()).await;

        let mut acl = container_proto::cluster::ServiceACLProto::default();
        protobuf::text::parse_text_proto(SERVICE_ACL_PROTO, &mut acl)?;

        let mut config_registry = ControllerConfigRegistry::defaults().await?;
        let config = config_registry.remove(&self.config_name)
            .ok_or_else(|| format_err!("No config named: {}", self.config_name))?;

        /*
        println!("Go!");
        motion_controller.move_to(vec3f(4.0 * 3200.0 / 80.0, 0.0, 0.0), (1600.0 / 80.0) * 2.0).await?;
        */

        let mut server = ClusterServer::new(self.port.value(), acl, client)?;

        let mut inst = Arc::new(ControllerServiceImpl::create(config).await?);
        // service.register_dependency(inst.clone()).await;
        server.add_service(inst.clone().into_service())?;

        service.register_dependency(server.start()?).await;


        println!("Ready!");
        service.wait().await
    }

}


