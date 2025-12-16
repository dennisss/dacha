use std::sync::Arc;
use std::time::Duration;
use std::f32::consts::PI;

use common::errors::*;
use math::matrix::{VectorXf, MatrixXd};
use math::vecxf;
use cluster_client::ClusterMetaClient;
use cnc_controller_proto::cnc::*;
use cnc_controller::motion_controller_sim::MotionControllerSimulator;
use cnc_controller::motion_controller::MotionController;
use cnc_controller::config::ControllerConfigRegistry;
use file::LocalPathBuf;
use file::project_path;
use cnc_controller::proto_utils::*;


pub struct RemoteMachineController {
    client: Arc<ClusterMetaClient>,
    stub: ControllerStub
}

impl RemoteMachineController {
    pub async fn create() -> Result<Self> {
        let client = ClusterMetaClient::create_from_environment().await?;

        let channel = cluster_client::service::create_rpc_channel(
            "localhost:8000", client.clone()).await?;
        // let channel = cluster_client::service::create_rpc_channel(
        //     "voron0.job.local.cluster.internal", client.clone()).await?;

        let stub = ControllerStub::new(channel);

        Ok(Self {
            client,
            stub
        })
    }

    pub async fn execute(&mut self, request: &ExecuteRequest) -> Result<ExecuteResponse> {
        let request_context = rpc::ClientRequestContext::default();
        self.stub.Execute(&request_context, request).await.result
    }

    pub async fn move_to(&mut self, pos: &VectorXf, feed_rate: f32) -> Result<()> {
        let request_context = rpc::ClientRequestContext::default();
        let mut request = ExecuteRequest::default();

        let cmd = request.new_commands();
        let m = cmd.move_to_mut();
        m.set_position(pos.to_proto());
        m.set_feed_rate(feed_rate);

        self.stub.Execute(&request_context, &request).await.result?;
        Ok(())
    }

    pub async fn move_towards_endstop(&mut self, pos: &VectorXf, feed_rate: f32) -> Result<Option<VectorXf>> {
        let request_context = rpc::ClientRequestContext::default();
        let mut request = ExecuteRequest::default();

        let cmd = request.new_commands();
        let m = cmd.move_to_mut();
        m.set_position(pos.to_proto());
        m.set_feed_rate(feed_rate);
        m.set_towards_endstop(true);

        let res = self.stub.Execute(&request_context, &request).await.result?;
        
        let mut out = None;
        if res.has_hit_position() {
            out = Some(VectorXf::from_proto(res.hit_position()));
        }

        Ok(out)
    }

    pub async fn wait_until_idle(&mut self) -> Result<()> {
        let request_context = rpc::ClientRequestContext::default();
        let mut request = ExecuteRequest::default();

        request.new_commands().set_wait_until_idle(true);

        self.stub.Execute(&request_context, &request).await.result?;
        Ok(())
    }

    pub async fn last_position(&mut self) -> Result<VectorXf> {
        let request_context = rpc::ClientRequestContext::default();
        let mut request = GetPositionRequest::default();

        let res = self.stub.GetPosition(&request_context, &request).await.result?;

        Ok(VectorXf::from_proto(res.position()))
    }


}