use std::time::Duration;

use base_error::*;
use executor_multitask::ServiceResource;
use cnc_monitor_proto::cnc::*;
use common::fixed::vec::FixedVec;
use common::bytes::Bytes;
use executor::channel::oneshot;

use crate::timestamped_value::*;

#[async_trait]
pub trait ConnectionController: 'static + Send + Sync + ServiceResource {

    async fn connected(&self) -> Result<bool>;
    async fn state_proto(&self, proto: &mut MachineStateProto) -> Result<()>;
    async fn axis_value(&self, axis_name: &str) -> Result<AxisData>;

    // tODO: Get rid of this and only use the enqueue version?
    async fn send_command_impl(&self, line: Bytes, timeout: Duration) -> Result<()>;

    async fn enqueue_command_impl(
        &self,
        line: Bytes,
        timeout: Duration,
    ) -> Result<PendingCommand>;

    async fn full_stop(&self) -> Result<()>;
    async fn wait_for_idle(&self) -> Result<()>;
    async fn get_current_axis_value(&self, axis_id: &str) -> Result<AxisData>;
    async fn tool_change(&self, tool_index: i32) -> Result<()>;

    async fn read_serial_log(
        &self,
        response: &mut rpc::ServerStreamResponse<'_, ReadSerialLogResponse>,
    ) -> Result<()>;

    async fn set_temperature(&self, axis_id: &str, target: f32) -> Result<()>;
    async fn home_x(&self) -> Result<()>;
    async fn home_y(&self) -> Result<()>;
    async fn home_all(&self) -> Result<()>;
    async fn mesh_level(&self) -> Result<()>;    
    async fn goto(&self, x: f32, y: f32, feed_rate: f32) -> Result<()>;
    async fn set_spindle_state(&self, state: &SpindleState) -> Result<()>;
    async fn goto3(&self, x: Option<f32>, y: Option<f32>, z: Option<f32>, feed_rate: f32) -> Result<()>;
    async fn request_state_update(&self) -> Result<()>;

}

pub trait ConnectionControllerExt: ConnectionController {
    async fn enqueue_command<D: Into<Bytes> + Send>(
        &self,
        line: D,
        timeout: Duration,
    ) -> Result<PendingCommand> {
        self.enqueue_command_impl(line.into(), timeout).await
    }

    async fn send_command<D: Into<Bytes> + Send>(&self, line: D, timeout: Duration) -> Result<()> {
        self.send_command_impl(line.into(), timeout).await
    }
}

impl ConnectionControllerExt for dyn ConnectionController {}
impl<T: ConnectionController> ConnectionControllerExt for T {}


#[derive(Clone)]
pub struct AxisData {
    /// Will be empty if no data has been collected yet.
    pub data: TimestampedValue<FixedVec<f32, 2>>,
}

#[derive(Clone, Debug, Fail)]
pub enum SendCommandError {
    ReceivedError(String),
    DeadlineExceeded,
    AbruptCancellation,
}

impl std::fmt::Display for SendCommandError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::result::Result<(), std::fmt::Error> {
        std::fmt::Debug::fmt(self, f)
    }
}

pub struct PendingCommand {
    receiver: oneshot::Receiver<Result<(), SendCommandError>>,
}

impl PendingCommand {
    pub fn new(receiver: oneshot::Receiver<Result<(), SendCommandError>>) -> Self {
        Self { receiver }
    }

    pub async fn wait(self) -> Result<(), SendCommandError> {
        self.receiver
            .recv()
            .await
            .map_err(|_| SendCommandError::AbruptCancellation)?
    }

    pub async fn ready(&self) -> bool {
        self.receiver.can_recv().await
    }
}
