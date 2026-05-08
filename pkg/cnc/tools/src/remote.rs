use std::sync::Arc;
use std::time::Duration;
use std::f32::consts::PI;

use common::errors::*;
use math::matrix::{VectorXd, MatrixXd};
use math::vecxd;
use cluster_client::ClusterMetaClient;
use cnc_controller_proto::cnc::*;
use file::LocalPathBuf;
use file::project_path;
use cnc_controller::proto_utils::*;
use executor::sync::AsyncMutex;
use executor_multitask::TaskResource;
use executor::lock;


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

    pub async fn home(&mut self) -> Result<()> {
        let request_context = rpc::ClientRequestContext::default();
        let mut request = ExecuteRequest::default();

        let cmd = request.new_commands();
        cmd.home_mut();
        self.stub.Execute(&request_context, &request).await.result?;
        Ok(())        
    }

    pub async fn move_to(&mut self, pos: &VectorXd, feed_rate: f64) -> Result<()> {
        let mut request = ExecuteRequest::default();

        let cmd = request.new_commands();
        let m = cmd.move_to_mut();
        m.set_position(pos.to_proto());
        m.options_mut().set_feed_rate(feed_rate);

        self.execute(&request).await?;
        Ok(())
    }

    pub async fn move_towards_endstop(&mut self, pos: &VectorXd, feed_rate: f64) -> Result<Option<VectorXd>> {
        let mut request = ExecuteRequest::default();

        let cmd = request.new_commands();
        let m = cmd.move_to_mut();
        m.set_position(pos.to_proto());
        m.options_mut().set_feed_rate(feed_rate);
        m.set_towards_endstop(true);

        let res = self.execute(&request).await?;
        
        let mut out = None;
        if res.has_hit_position() {
            out = Some(VectorXd::from_proto(res.hit_position()));
        }

        Ok(out)
    }

    pub async fn set_servo_position(&mut self, pos: f32) -> Result<()> {
        let mut request = ExecuteRequest::default();
        let cmd = request.new_commands();
        cmd.set_servo_position_mut().set_position(pos);

        self.execute(&request).await?;

        Ok(())
    }

    pub async fn wait_until_idle(&mut self) -> Result<()> {
        let request_context = rpc::ClientRequestContext::default();
        let mut request = ExecuteRequest::default();

        request.new_commands().set_wait_until_idle(true);

        self.stub.Execute(&request_context, &request).await.result?;
        Ok(())
    }

    pub async fn last_position(&mut self) -> Result<VectorXd> {
        let request_context = rpc::ClientRequestContext::default();
        let mut request = GetLastPositionRequest::default();

        let res = self.stub.GetLastPosition(&request_context, &request).await.result?;

        Ok(VectorXd::from_proto(res.position()))
    }

    pub async fn read_log(
        &mut self
    ) -> Result<LogDataCollector> {
        let request_context = rpc::ClientRequestContext::default();
        let mut request = ReadLogRequest::default();

        let mut res = self.stub.ReadLog(&request_context, &request).await;

        res.recv_head().await;

        let shared = Arc::new(LogDataShared::default());

        let task_resource = TaskResource::spawn_interruptable(
            "log_collector",
            LogDataCollector::background_thread(shared.clone(), res),
        );

        Ok(LogDataCollector {
            task_resource, shared
        })
    }
}

// Used to hold all log entries that have been received so far.
pub struct LogDataCollector {
    shared: Arc<LogDataShared>,
    task_resource: TaskResource,
}

#[derive(Default)]
struct LogDataShared {
    data: AsyncMutex<Vec<LogEntry>>,
}

impl LogDataCollector {
    async fn background_thread(
        shared: Arc<LogDataShared>,
        mut res: rpc::ClientStreamingResponse<ReadLogResponse>,
    ) -> Result<()> {
        while let Some(entry) = res.recv().await {
            lock!(data <= shared.data.lock().await?, {
                data.push(entry.entry().clone());
            });
        }

        res.finish().await?;

        Err(err_msg("Stopped receiving logs"))
    }

    pub async fn drain(&self) -> Result<Vec<LogEntry>> {
        let mut out = vec![];

        lock!(data <= self.shared.data.lock().await?, {
            core::mem::swap(&mut *data, &mut out);
        });

        Ok(out)
    }
}


