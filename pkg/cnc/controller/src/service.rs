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
use math::matrix::vec3f;
use executor_multitask::RootResource;
use cluster_client::ClusterMetaClient;
use cluster_client::ClusterServer;
use cnc_controller_proto::cnc::*;
use rpc_util::NamedPortArg;
use file::LocalPathBuf;

use crate::devices::*;
use crate::config::*;
use crate::motion_controller::*;

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
    devices: Arc<DevicesController>,
    motion_controller: Arc<MotionController>,

}

impl ControllerServiceImpl {
    async fn create(config: ControllerConfig) -> Result<Self> {

        let devices = DevicesController::create(&config).await?;

        println!("Wait for sync...");
        devices.time().wait_for_sync().await?;

        let motion_controller = Arc::new(MotionController::create(
            config.motion_controller().clone(), devices.clone()).await?);

        motion_controller.toggle_motors(true).await?;


        Ok(Self {
            devices,
            motion_controller
        })
    }

    async fn execute_impl(&self, request: &ExecuteRequest) -> Result<()> {

        for cmd in request.move_to() {
            self.motion_controller.move_to(vec3f(cmd.x(), cmd.y(), cmd.z()), (1600.0 / 80.0) * 2.0).await?;
        }

        Ok(())
    }

}

#[async_trait]
impl ControllerService for ControllerServiceImpl {
    async fn Execute(
        &self,
        request: rpc::ServerRequest<ExecuteRequest>,
        response: &mut rpc::ServerResponse<ExecuteResponse>,
    ) -> Result<()> {
        self.execute_impl(&request.value).await?;
        // response.value = ...;
        Ok(())
    }
}

use cnc::quadratic_stepper_motion::QuadraticStepperMotion;
use common::fixed::queue::FixedQueue;

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

#[derive(Args)]
pub struct ExecuteCommand {
    // #[arg(positional)]
    proto: Option<String>,

    gcode_file: Option<LocalPathBuf>,
}

impl ExecuteCommand {
    pub async fn run(self) -> Result<()> {
        let client = ClusterMetaClient::create_from_environment().await?;

        let channel = cluster_client::service::create_rpc_channel(
            "localhost:8000", client.clone()).await?;

        let stub = ControllerStub::new(channel);
        let request_context = rpc::ClientRequestContext::default();


        if let Some(path) = self.gcode_file {

            let cmds = Self::get_all_gcode_commands(&path).await?;

            let mut last_pos = vec![0.0, 0.0, 0.0];

            for cmd in cmds {



                match cmd {
                    gcode::Command::SetBedTemperatureAndWaitCommand(_) => {

                    }
                    gcode::Command::SetExtruderTemperature(_) => {

                    }
                    gcode::Command::SetExtruderTemperatureAndWait(_) => {

                    }
                    gcode::Command::SetUnitsToMillimeters(_) => {

                    }
                    gcode::Command::SetToAbsoluteMode(_) => {

                    }
                    gcode::Command::SetExtruderToRelativeMode(_) => {

                    }
                    gcode::Command::FanOn(_) => {

                    }

                    // LinearMove(LinearMove { inner: Move { x: Some(51.312), y: Some(57.403), z: None, e: Some(0.29417), feed_rate: None } })
                    gcode::Command::LinearMove(cmd) => {

                        // TODO: Use deref and get rid of the inners.
                        if let Some(v) = cmd.inner.x {
                            last_pos[0] = v.to_f32();
                        }
                        if let Some(v) = cmd.inner.y {
                            last_pos[1] = v.to_f32();
                        }
                        if let Some(v) = cmd.inner.z {
                            last_pos[2] = v.to_f32();
                        }

                        let mut request = ExecuteRequest::default();
                        let move_to = request.new_move_to();
                        move_to.set_x(last_pos[0]);
                        move_to.set_y(last_pos[1]);
                        move_to.set_z(last_pos[2]);

                        stub.Execute(&request_context, &request).await.result?;

                    }
                    gcode::Command::FanOff(_) => {

                    }
                    gcode::Command::SetDefaultAcceleration(_) => {

                    }
                    gcode::Command::SetPosition(_) => {
                        // TODO: This is used for setting extruder position to zero.
                    }
                    c @ _ => {
                        eprintln!("Uknown: {:?}", c);
                    }

                }

            }


        }

        if let Some(proto) = &self.proto {
            let mut request = ExecuteRequest::default();
            protobuf::text::parse_text_proto(&proto, &mut request)?;
            println!("{:?}", stub.Execute(&request_context, &request).await.result?);
        }

        Ok(())
    }

    async fn get_all_gcode_commands(path: &file::LocalPath) -> Result<Vec<gcode::Command>> {
        let data = file::read(&path).await?;

        let mut parser = gcode::ProgramParser::default();
        let mut remaining = &data[..];

        let mut commands = vec![];

        let mut els = vec![];
        while !remaining.is_empty() {
            els.clear();
            let nread = parser.parse_line(remaining, true, &mut els);
            remaining = &remaining[nread..];

            let mut command = None;

            for el in els.drain(..) {
                match el {
                    gcode::ProgramElement::Command(c) => {
                        if command.is_some() {
                            return Err(err_msg("Multi-command line"));
                        }

                        command = Some(c);
                    }
                    gcode::ProgramElement::Error(e) => {
                        return Err(format_err!("Error while parsing gcode line: {}", e));
                    }
                    gcode::ProgramElement::EndOfLine |
                    gcode::ProgramElement::Thumbnail(_) |
                    gcode::ProgramElement::Metadata { .. } => {},
                }
            }

            if let Some(command) = command {
                commands.push(command);
                
                // println!("{:?}", command);
            }
        }

        Ok(commands)
    }

}




