use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};
use std::collections::HashMap;

use base_error::*;
use common::bytes::Bytes;
use common::hash::FastHasherBuilder;
use executor::channel::oneshot;
use executor::sync::{AsyncMutex, AsyncRwLock, AsyncVariable, SyncMutex};
use executor_multitask::{impl_resource_passthrough, ServiceResourceGroup};
use cnc_monitor_proto::cnc::*;
use executor::lock;
use cluster_client::ClusterMetaClient;
use cnc_controller_proto::cnc::*;
use math::vecxd;
use math::matrix::VectorXd;
use cnc_controller::proto_utils::VectorProtoExt;

use crate::timestamped_value::TimestampedValue;
use crate::connection_controller::*;
use crate::config::*;
use crate::change::*;
use crate::metric::*;

// TODO: Need a lot more locking somewhere to make sure concurrent callers aren't messing each other up.

pub struct RpcConnectionController {
    resources: ServiceResourceGroup,
    shared: Arc<Shared>,
}

impl_resource_passthrough!(RpcConnectionController, resources);

struct Shared {
    machine_id: u64,
    config: Arc<AsyncRwLock<MachineConfigContainer>>,
    state: AsyncMutex<State>,
    change_publisher: ChangePublisher,
    stub: ControllerStub,
    axis_metrics: HashMap<String, Vec<MetricStream>, FastHasherBuilder>,
}

struct State {
    connected: bool,
    axes: HashMap<String, AxisData, FastHasherBuilder>,

    // TODO: We need to update the position stored in here if it is changed by other stuff.
    // TODO: Move stuff that we need out of the 'cnc_tools' crate so that we don't have a dependency on 'cnc_controller'
    gcode_state: cnc_tools::gcode::CommandConverter,
}

impl RpcConnectionController {

    pub async fn create(
        machine_id: u64,
        config: Arc<AsyncRwLock<MachineConfigContainer>>,
        change_publisher: ChangePublisher,
        metric_store: &MetricStore,
        meta_client: Arc<ClusterMetaClient>,
        addr: &str,
    ) -> Result<Self> {
        let resources = ServiceResourceGroup::new("cnc::RpcMachine");

        // TODO: Dynamic address
        let channel = cluster_client::service::create_rpc_channel(
            addr, meta_client.clone()).await?;

        let stub = ControllerStub::new(channel);

        let mut state = State {
            connected: false,
            axes: HashMap::default(),
            // TODO: Make this more dynamic.
            gcode_state: cnc_tools::gcode::CommandConverter::new(vecxd!(0., 0., 0., 0.))
        };

        let mut axis_metrics = HashMap::default();

        // TODO: Dedup with SerialController.
        let config_value = config.read().await?;
        for axis_config in config_value.axes() {
            state.axes.insert(
                axis_config.id().to_string(),
                AxisData {
                    data: TimestampedValue::default(),
                },
            );

            let mut num_values = {
                // TODO: THis should also be 2 for switches if they are controllable.
                if axis_config.typ() == AxisType::HEATER {
                    2
                } else {
                    1
                }
            };

            if axis_config.has_collect() {
                let mut streams = vec![];
                for i in 0..num_values {
                    let mut resource = MetricResource::default();
                    resource.set_machine_id(machine_id);
                    resource.set_kind(MetricKind::MACHINE_AXIS_VALUE);
                    resource.set_axis_id(axis_config.id());
                    resource.set_value_index(i as u32);

                    let stream = metric_store.stream(&resource).await?;
                    streams.push(stream);
                }

                axis_metrics.insert(axis_config.id().to_string(), streams);
            }
        }

        drop(config_value);




        let shared = Arc::new(Shared {
            machine_id,
            config,
            state: AsyncMutex::new(state),
            change_publisher,
            stub,
            axis_metrics,
        });

        resources.spawn_interruptable("RpcPoller", Self::state_poller(shared.clone())).await;

        Ok(Self {
            resources,
            shared
        })
    }

    // TODO: Make all RPCs fail if the RpcConnectionController is being terminated.
    async fn execute(&self, request: &ExecuteRequest) -> Result<ExecuteResponse> {
        let request_context = rpc::ClientRequestContext::default();
        self.shared.stub.Execute(&request_context, request).await.result
    }

    async fn last_position(&self) -> Result<VectorXd> {
        let request_context = rpc::ClientRequestContext::default();
        let mut request = GetLastPositionRequest::default();

        let res = self.shared.stub.GetLastPosition(&request_context, &request).await.result?;

        Ok(VectorXd::from_proto(res.position()))
    }

    async fn move_to(&self, pos: &VectorXd, feed_rate: f64) -> Result<()> {
        let mut request = ExecuteRequest::default();

        let cmd = request.new_commands();
        let m = cmd.move_to_mut();
        m.set_position(pos.to_proto());
        m.options_mut().set_feed_rate(feed_rate);

        self.execute(&request).await?;
        Ok(())
    }


    async fn state_poller(shared: Arc<Shared>) -> Result<()> {

        // TODO: Wait for ready
        let request_context = rpc::ClientRequestContext::default();
        let mut request = GetStateRequest::default();

        loop {
            let proto = shared.stub.GetState(&request_context, &request).await.result?;

            let now = Instant::now();
            let now_systime = SystemTime::now();
            let config = shared.config.read().await?;

            let mut new_axes = HashMap::<String, AxisData, FastHasherBuilder>::default();

            for (i, id) in ["X", "Y", "Z", "E"].into_iter().enumerate() {
                new_axes.insert(
                    id.to_string(),
                    AxisData {
                        data: TimestampedValue::new((&[
                            proto.position().values()[i] as f32,
                        ][..]).into(), now),
                    },
                );
            }

            new_axes.insert(
                "T".into(),
                AxisData {
                    data: TimestampedValue::new((&[
                        proto.heater_temp(),
                        proto.heater_target(),
                    ][..]).into(), now),
                },
            );

            // TODO: Dedup with the serial controller.
            for (axis, axis_data) in new_axes.iter() {
                let axis_config = config
                    .axes_map()
                    .get(axis)
                    .ok_or_else(|| format_err!("Missing axis config: {}", axis))?;

                if !axis_config.has_collect() {
                    continue;
                }

                let streams = shared.axis_metrics.get_or_err(axis)?;

                for (i, value) in axis_data
                    .data
                    .get()
                    .map(|d| d.as_ref())
                    .unwrap_or(&[])
                    .iter()
                    .cloned()
                    .enumerate()
                {
                    if axis_config.collect().has_min_value() {
                        if value < axis_config.collect().min_value() {
                            continue;
                        }
                    }

                    let stream = streams
                        .get(i)
                        .ok_or_else(|| err_msg("Wrong number of stream metrics for axis"))?;

                    // TODO: Instead use the axis_data timestamp / the line timestamp.
                    stream.record(now_systime, value).await?;
                }
            }

            let was_connected = lock!(state <= shared.state.lock().await?, {
                state.axes.extend(new_axes.drain());

                let was_connected = state.connected;
                state.connected = true;
                was_connected
            });

            shared.change_publisher.publish(ChangeEvent::new(
                EntityType::MACHINE,
                Some(shared.machine_id),
                was_connected,
            ));

            executor::sleep(Duration::from_secs(1)).await?;
        }

        Ok(())
    }


}

#[async_trait]
impl ConnectionController for RpcConnectionController {

    async fn connected(&self) -> Result<bool> {
        lock!(state <= self.shared.state.lock().await?, {
            Ok(state.connected)
        })
    }

    async fn state_proto(&self, proto: &mut MachineStateProto) -> Result<()> {
        // TODO: Dedup with SerialController.

        let state = self.shared.state.lock().await?.read_exclusive();
        if !state.connected {
            proto.set_connection_state(MachineStateProto_ConnectionState::CONNECTING);
            return Ok(());
        }

        proto.set_connection_state(MachineStateProto_ConnectionState::CONNECTED);

        for (axis_id, axis) in &state.axes {
            let proto = proto.new_axis_values();
            proto.set_id(axis_id);
            if let Some(value) = axis.data.get() {
                proto.value_mut().extend_from_slice(&value[..]);
            }
        }

        Ok(())
    }

    async fn axis_value(&self, axis_name: &str) -> Result<AxisData> {
        // TODO: Dedup with SerialController.

        let state = self.shared.state.lock().await?.read_exclusive();

        state
            .axes
            .get(axis_name)
            .cloned()
            .ok_or_else(|| err_msg("Missing axis"))
    }

    async fn get_current_axis_value(&self, axis_id: &str) -> Result<AxisData> {
        // TODO: Dedup this with the SerialController
        
        let now = Instant::now();

        loop {
            let state = self.shared.state.lock().await?.read_exclusive();
            if !state.connected {
                return Err(rpc::Status::failed_precondition("Machine not connected").into());
            }

            let data = state
                .axes
                .get(axis_id)
                .ok_or_else(|| err_msg("Missing axis data"))?;
            let last_updated = data
                .data
                .last_updated()
                .ok_or_else(|| err_msg("Data missing last update time"))?;

            if last_updated < now {
                drop(state);
                executor::sleep(Duration::from_millis(500)).await?;
                continue;
            }

            return Ok(data.clone());
        }
    }

    // TODO: Ideally 

    // TODO: Implement the timeout.
    // tODO: Get rid of this and only use the enqueue version?
    async fn send_command_impl(&self, line: Bytes, timeout: Duration) -> Result<()> {
        let mut gcode_commands = cnc_tools::gcode::parse_gcode_string(&line)?;

        let mut commands = vec![];

        lock!(state <= self.shared.state.lock().await?, {
            for cmd in gcode_commands {
                state.gcode_state.next(&cmd, &mut commands)?;
            }

            Result::<_, Error>::Ok(())
        })?;

        if !commands.is_empty() {
            let mut request = ExecuteRequest::default();
            for cmd in commands {
                request.add_commands(cmd);
            }

            self.execute(&request).await?;
        }

        Ok(())
    }

    async fn enqueue_command_impl(
        &self,
        line: Bytes,
        timeout: Duration,
    ) -> Result<PendingCommand> {
        // TODO: Have a proper enqueue (probably need to wait for the heeader from the server).

        self.send_command_impl(line, timeout).await?;

        let (sender, receiver) = oneshot::channel();
        sender.send(Ok(()));
        Ok(PendingCommand::new(receiver))
    }

    async fn full_stop(&self) -> Result<()> {
        Err(err_msg("full_stop not supported"))       
    }

    async fn wait_for_idle(&self) -> Result<()> {
        let mut request = ExecuteRequest::default();
        let cmd = request.new_commands();
        cmd.set_wait_until_idle(true);
        self.execute(&request).await?;
        Ok(())
    }

    async fn tool_change(&self, tool_index: i32) -> Result<()> {
        Err(err_msg("tool_change not supported"))       
    }

    async fn read_serial_log(
        &self,
        response: &mut rpc::ServerStreamResponse<'_, ReadSerialLogResponse>,
    ) -> Result<()> {
        Err(err_msg("read_serial_log not supported"))       
    }

    async fn set_temperature(&self, axis_id: &str, target: f32) -> Result<()> {
        let mut request = ExecuteRequest::default();
        let cmd = request.new_commands();
        cmd.set_temp_mut().set_target(target);
        self.execute(&request).await?;
        Ok(())
    }
    
    async fn home_all(&self) -> Result<()> {
        let mut request = ExecuteRequest::default();
        let cmd = request.new_commands();
        cmd.home_mut();
        self.execute(&request).await?;
        Ok(())
    }

    async fn home_x(&self) -> Result<()> {
        Err(err_msg("home_x not supported"))
    }
    
    async fn home_y(&self) -> Result<()> {
        Err(err_msg("home_y not supported"))
    }
    
    async fn mesh_level(&self) -> Result<()> {
        Err(err_msg("mesh_level not supported"))
    }
    
    async fn goto(&self, x: f32, y: f32, feed_rate: f32) -> Result<()> {
        let mut pos = self.last_position().await?;
        pos[0] = x as f64;
        pos[1] = y as f64;
        self.move_to(&pos, feed_rate as f64).await?;
        Ok(())
    }
    
    async fn goto3(&self, x: Option<f32>, y: Option<f32>, z: Option<f32>, feed_rate: f32) -> Result<()> {
        let mut pos = self.last_position().await?;
        if let Some(x) = x {
            pos[0] = x as f64;
        }
        if let Some(y) = y {
            pos[1] = y as f64;
        }
        if let Some(z) = z {
            pos[2] = z as f64;
        }

        self.move_to(&pos, feed_rate as f64).await?;
        Ok(())
    }
    
    async fn set_spindle_state(&self, state: &SpindleState) -> Result<()> {
        Err(err_msg("set_spindle_state not supported"))       
    }
        
    async fn request_state_update(&self) -> Result<()> {
        // TODO:
        Ok(())
    }

}