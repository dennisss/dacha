use std::sync::Arc;
use std::time::{Duration, Instant};

use common::errors::*;
use math::vecxd;
use executor::child_task::ChildTask;
use executor::channel;
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
    shared: Arc<Shared>
}

struct Shared {
    machine: MachineController
}

impl ControllerServiceImpl {
    async fn create(config: ControllerConfig) -> Result<Self> {
        let machine = MachineController::create(config).await?;
        Ok(Self { shared: Arc::new(Shared { machine }) })
    }

    async fn execute_reader(
        shared: Arc<Shared>,
        mut req_stream: rpc::ServerStreamRequest<ExecuteRequest>,
        responses: channel::Sender<Result<ExecuteResponse>>,
    ) -> Result<()> {
        while let Some(req) = req_stream.recv().await? {
            let (res, waiter) = shared.machine.execute(&req).await?;

            let responses = responses.clone();

            executor::spawn(async move {

                // TODO: Error handling and ensure this eventually terminates.
                if let Some(waiter) = waiter {
                    if let Ok(finish_time) = waiter.recv().await {
                        let now = Instant::now();
                        if now < finish_time {
                            executor::sleep(finish_time - now).await;
                        }
                    }
                }

                let _ = responses.send(Ok(res)).await;

            });
        }

        Ok(())
    }

}

#[async_trait]
impl ControllerService for ControllerServiceImpl {
    async fn GetState(
        &self,
        request: rpc::ServerRequest<GetStateRequest>,
        response: &mut rpc::ServerResponse<GetStateResponse>,
    ) -> Result<()> {
        response.value = self.shared.machine.get_state().await?;
        Ok(())
    }

    async fn Execute(
        &self,
        request: rpc::ServerRequest<ExecuteRequest>,
        response: &mut rpc::ServerResponse<ExecuteResponse>,
    ) -> Result<()> {
        response.value = self.shared.machine.execute(&request).await?.0;
        Ok(())
    }

    async fn ExecuteStream(
        &self,
        request: rpc::ServerStreamRequest<ExecuteRequest>,
        response: &mut rpc::ServerStreamResponse<ExecuteResponse>,
    ) -> Result<()> {

        let (sender, receiver) = channel::unbounded();

        let shared = self.shared.clone();
        let thread = ChildTask::spawn(async move {
            if let Err(e) = Self::execute_reader(shared, request, sender.clone()).await {
                let _ = sender.send(Err(e)).await;
            }
        });

        loop {
            let res = receiver.recv().await??;
            response.send(res).await?;
        }
    }

    async fn GetLastPosition(
        &self,
        request: rpc::ServerRequest<GetLastPositionRequest>,
        response: &mut rpc::ServerResponse<GetLastPositionResponse>,
    ) -> Result<()> {
        response.value = self.shared.machine.get_last_position().await?;
        Ok(())
    }

    // TODO: Currently if there are multiple callers to this, they will get
    // different subsets of the log data.
    async fn ReadLog(
        &self,
        request: rpc::ServerRequest<ReadLogRequest>,
        response: &mut rpc::ServerStreamResponse<ReadLogResponse>
    ) -> Result<()> {
        let mut subscriber = self.shared.machine.subscribe_to_log();

        response.send_head().await?;

        loop {
            let entry = subscriber.recv().await?;
            
            let mut res = ReadLogResponse::default();
            res.set_entry(entry.as_ref().clone());
            response.send(res).await?;
        }

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

        let mut acl = cluster_proto::cluster::ServiceACLProto::default();
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


