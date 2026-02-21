use std::sync::Arc;
use std::time::Duration;

use common::errors::*;
use math::matrix::VectorXd;
use math::vecxd;
use executor_multitask::RootResource;
use cluster_client::ClusterMetaClient;
use cluster_client::ClusterServer;
use cnc_controller_proto::cnc::*;
use rpc_util::NamedPortArg;
use file::LocalPathBuf;
use executor::channel;

use crate::devices::*;
use crate::config::*;
use crate::motion_controller::*;
use crate::heater_controller::*;
use crate::endstop_controller::*;
use crate::proto_utils::VectorProtoExt;
use crate::data_logger::*;


// TODO: Need to block most commands until we are homed.

pub struct MachineController {
    config: ControllerConfig,
    devices: Arc<DevicesController>,
    motion_controller: Arc<MotionController>,
    endstop_controller: Arc<EndstopController>,
    heater_controllers: Vec<Arc<HeaterController>>,
    loggers: Vec<Arc<DataLogger>>,
    log_receiver: channel::Receiver<LogEntry>,
}

impl MachineController {
    pub async fn create(config: ControllerConfig) -> Result<Self> {

        let devices = DevicesController::create(&config).await?;

        println!("Wait for sync...");
        devices.time().wait_for_sync().await?;

        let (log_sender, log_receiver) = channel::bounded(512);

        let motion_controller = Arc::new(MotionController::create(
            config.motion_controller().clone(), devices.clone(), log_sender.clone()).await?);

        motion_controller.enable(true).await?;

        let endstop_controller = Arc::new(EndstopController::create(
            config.endstop_controller().clone(),
            motion_controller.clone(),
            devices.clone()
        ).await?);
        // In default modes, all endstops will trigger an alarm.
        endstop_controller.start(config.default_endstops(), &[]).await?;


        let mut heater_controllers = vec![];
        for config in config.heater_controllers() {
            heater_controllers.push(
                Arc::new(
                    HeaterController::create(devices.clone(), config.as_ref().clone()))
            );
        }

        let mut loggers = vec![];
        for config in config.loggers() {
            loggers.push(Arc::new(DataLogger::create(config, devices.clone(), log_sender.clone())?));
        }

        Ok(Self {
            config,
            devices,
            motion_controller,
            endstop_controller,
            heater_controllers,
            loggers,
            log_receiver,
        })
    }

    pub async fn get_position(&self) -> Result<GetPositionResponse> {
        let mut res = GetPositionResponse::default();
        res.set_position(self.motion_controller.last_position().await?.to_proto());
        Ok(res)
    }

    pub async fn execute(&self, request: &ExecuteRequest) -> Result<ExecuteResponse> {
        let mut res = ExecuteResponse::default();

        /*
        // TODO: At most one execute request should be allowed to run at a time.

    pub fn set_max_junction_deviation(&mut self, value: f64) {
        self.config.set_max_junction_deviation(value);
    }
        */

        for cmd in request.commands() {
            if cmd.has_configure() {
                self.motion_controller.set_max_junction_deviation(cmd.configure().max_junction_deviation()).await?;
            }

            if cmd.has_set_temp() {
                let cmd = cmd.set_temp();
                self.heater_controllers[0].set_target_temperature(cmd.target()).await?;
            }

            if cmd.has_move_to() {
                let cmd = cmd.move_to();

                let v = {
                    if cmd.has_position() {
                        // TODO: Make sure this has checks on number of axes
                        VectorXd::from_proto(cmd.position())
                    } else {
                        let mut points = vec![cmd.x(), cmd.y(), cmd.z(), cmd.e()];
                        points.truncate(self.motion_controller.num_axes());
                        VectorXd::from_slice_with_shape(points.len(), 1, &points)
                    }
                };

                if cmd.towards_endstop() {
                    self.move_towards_endstop(v, cmd.feed_rate()).await?;

                    // NOTE: Hit position is only available for timed endstops.
                    let hit_position = self.motion_controller.hit_position().await?;

                    if let Some(pos) = hit_position {
                        res.set_hit_position(pos.to_proto());
                    }

                } else {
                    self.motion_controller.move_to(v, cmd.feed_rate()).await?;
                }
            }

            if cmd.has_set_position() {
                println!("SET POSITION!!");

                self.motion_controller.wait_until_idle().await?;

                let cmd = cmd.set_position();
                let v = VectorXd::from_proto(cmd.position());

                self.motion_controller.set_position(v).await?;

            }

            if cmd.reset_alarm() {
                // TODO: Only do this stuff if currently in the alarm state.

                self.motion_controller.reset_alarm().await?;

                self.motion_controller.enable(true).await?;

                self.endstop_controller.start(self.config.default_endstops(), &[]).await?;

            }

            if cmd.wait_until_idle() {
                self.motion_controller.wait_until_idle().await?;
            }

            if cmd.has_home() {

                // TODO: After homing, set the endstops back to their default state.

                let feed_rate = 20.0;

                let num_axes = self.motion_controller.num_axes();

                // TODO: Make everything zero'ed

                self.motion_controller.wait_until_idle().await?;

                let zero = VectorXd::zero_with_shape(num_axes, 1);
                self.motion_controller.set_position(zero.clone()).await?;

                println!("Zeroed!");

                let mut current_pos = zero.clone();

                // Lift in Z
                current_pos[2] = 10.0;
                self.motion_controller.move_to(current_pos.clone(), 10.0).await?;

                for axis_i in 0..2 {
                    println!("Raming min {}!", axis_i);
                    {
                        current_pos[axis_i] = -200.0;
                        self.move_towards_endstop(current_pos.clone(), feed_rate).await?;
                    }

                    // tODO: Must verify we actually hit the expected endstops (and reset to using all normal endstops.)

                    println!("Ramped min {}!", axis_i);

                    current_pos[axis_i] = 0.0;
                    self.motion_controller.set_position(current_pos.clone()).await?;

                    println!("Backing off");
                    {
                        current_pos[axis_i] = 20.0;
                        self.motion_controller.move_to(current_pos.clone(), feed_rate).await?;
                    }

                    // TODO: Issue is that after any move we may enter estop mode again so ideally wait_until_idle
                    // returns an error in that case if we aren't expecting it (or we are expecting it but didn't get it)

                }

                /*
                Some notes:
                - Handling long running events:
                    - Suppose I care about the the ADC event but then I don't
                    - Send the request
                    - MCU will keep track of the sequence number in the peripheral
                    - Later we can send a 'cancel' (sequence_number) request
                        - Once an ack is received for this request, the MCU won't use that sequence number anymore
                        - 


                Homing Z:
                - Move to the center
                - Wait for idle
                - Need a background thread constantly polling for the ADC event.
                    - Mainly need to hope that the eventdoesn't happen 


                */

                {
                    current_pos[0] = 60.0;
                    current_pos[1] = 60.0;

                    self.motion_controller.move_to(current_pos.clone(), feed_rate).await?;
                    self.motion_controller.wait_until_idle().await?;
                }

                // Need time for the toolhead to stop shaking.
                executor::sleep(Duration::from_millis(500)).await?;

                println!("GO Z!!!");

                current_pos[2] = -200.0;
                self.move_towards_endstop(current_pos.clone(), 10.0).await?;

                // TODO: Eventually rely on better timing data about hits to improve this.
                current_pos[2] = 0.0;
                self.motion_controller.set_position(current_pos.clone()).await?;

                current_pos[2] = 10.0;
                self.motion_controller.move_to(current_pos.clone(), 10.0).await?;
            }

            if cmd.has_set_fan_speed() {
                // TODO: This needs to be syncronized with motions.

                let cmd = cmd.set_fan_speed();

                // TODO: Batch these
                for fan_config in self.config.fans() {
                    if !cmd.name().is_empty() {
                        if fan_config.name() != cmd.name() {
                            continue;
                        }
                    } else if fan_config.group_index() != 1 {
                        continue;
                    }

                    let device = self.devices.get_peripherals_device(
                        fan_config.speed_control().device_name()).await?;

                    device.pwm_write(
                        fan_config.speed_control().peripheral_name(),
                        cmd.speed()
                    ).await?;
                }
            }

            if cmd.has_set_servo_position() {
                let cmd = cmd.set_servo_position();

                // TODO: Read from the peripheral
                let frequency = 200.0;
                let period = 1.0 / frequency;

                let pulse_time = cmd.position() / 1000000.0;

                // pulse_time = period * duty_cycle
                // So 'duty_cycle = pulse_time / period'
                let duty_cycle = pulse_time / period;

                for servo in self.config.servos() {
                    // if servo.name() != 

                    let device = self.devices.get_peripherals_device(
                        servo.pwm().device_name()).await?;

                    device.pwm_write(
                        servo.pwm().peripheral_name(),
                        duty_cycle
                    ).await?;
                }
            }
        }

        Ok(res)
    }

    // TODO: Failure of this should trigger an alarm (even if done by a remote script).
    async fn move_towards_endstop(
        &self,
        position: VectorXd,
        feed_rate: f64,
    ) -> Result<()> {
        if position.len() != self.config.motion_controller().axes().len() {
            return Err(err_msg("Wrong number of dimensions in position"));
        } 

        // Must be idle before we change the endstops.
        self.motion_controller.wait_until_idle().await?;

        let last_position = self.motion_controller.last_position().await?;
        
        let mut selected_set_i = None;

        for i in 0..position.len() {
            let delta = position[i] - last_position[i];

            if delta.abs() < 0.001 {
                continue;
            }

            let axis_name = self.config.motion_controller().axes()[i].name();

            let mut found = false;

            for (i, endstop_set) in self.config.endstop_sets().iter().enumerate() {
                if endstop_set.axis().iter().find(|a| a == &axis_name).is_none() {
                    continue;
                }

                if delta > 0.0 && !endstop_set.max_limit() {
                    continue;
                }

                if delta < 0.0 && !endstop_set.min_limit() {
                    continue;
                }

                if let Some(last_i) = selected_set_i {
                    if last_i != i {
                        return Err(err_msg("Overlapping endstop sets selected"));
                    }
                }

                selected_set_i = Some(i);
            }

            if selected_set_i.is_none() {
                return Err(format_err!("No endstop set to capture motion in axis: {}", axis_name))
            }
        }

        let selected_set_i = selected_set_i
            .ok_or_else(|| err_msg("Unable to resolve an endstop set for motion"))?;

        let endstop_set = &self.config.endstop_sets()[selected_set_i];

        self.endstop_controller.start(
            endstop_set.monitored_endstops(),
            endstop_set.expected_endstops()
        ).await?;

        self.motion_controller.move_to(position, feed_rate).await?;
        self.motion_controller.wait_until_idle().await?;

        // TODO: This only checks that we hit an endstop but doesn't verify that
        // the motion stopped due to that endstop (and not due to the motion finishing
        // naturally)
        self.endstop_controller.check_hit_something().await?;

        // Reset back to default endstops.
        // TODO: Ensure this always happens even if the above code fails?
        self.endstop_controller.start(self.config.default_endstops(), &[]).await?;

        Ok(())
    }


    pub fn clear_log(&self) -> Result<()> {
        while let Ok(_) = self.log_receiver.try_recv() {}
        Ok(())
    }

    pub async fn recv_log_entry(&self) -> Result<LogEntry> {
        let entry = self.log_receiver.recv().await?;
        Ok(entry)
    }

}
