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
use crate::heater_controller::*;
use crate::gcode::CommandConverter;
use crate::endstop_controller::*;


pub struct MachineController {
    config: ControllerConfig,
    devices: Arc<DevicesController>,
    motion_controller: Arc<MotionController>,
    endstop_controller: Arc<EndstopController>,
    heater_controllers: Vec<Arc<HeaterController>>,
}

impl MachineController {
    pub async fn create(config: ControllerConfig) -> Result<Self> {

        let devices = DevicesController::create(&config).await?;

        println!("Wait for sync...");
        devices.time().wait_for_sync().await?;

        let motion_controller = Arc::new(MotionController::create(
            config.motion_controller().clone(), devices.clone()).await?);

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

        Ok(Self {
            config,
            devices,
            motion_controller,
            endstop_controller,
            heater_controllers
        })
    }

    pub async fn execute(&self, request: &ExecuteRequest) -> Result<()> {
        /*
        // TODO: At most one execute request should be allowed to run at a time.

    pub fn set_max_junction_deviation(&mut self, value: f32) {
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

                let mut points = vec![cmd.x(), cmd.y(), cmd.z(), cmd.e()];
                points.truncate(self.motion_controller.num_axes());

                let v = VectorXf::from_slice_with_shape(points.len(), 1, &points);

                self.motion_controller.move_to(v, cmd.feed_rate()).await?;
            }

            if cmd.has_set_position() {
                println!("SET POSITION!!");

                self.motion_controller.wait_until_idle().await?;

                let cmd = cmd.set_position();

                let mut points = vec![cmd.x(), cmd.y(), cmd.z(), cmd.e()];
                points.truncate(self.motion_controller.num_axes());

                let v = VectorXf::from_slice_with_shape(points.len(), 1, &points);

                self.motion_controller.set_position(v).await?;

            }

            if cmd.reset_alarm() {
                // TODO: Only do this stuff if currently in the alarm state.

                self.motion_controller.reset_alarm().await?;

                self.motion_controller.enable(true).await?;

                self.endstop_controller.start(self.config.default_endstops(), &[]).await?;

            }

            if cmd.has_home() {

                // TODO: After homing, set the endstops back to their default state.

                let feed_rate = 20.0;

                let num_axes = self.motion_controller.num_axes();

                // TODO: Make everything zero'ed

                self.motion_controller.wait_until_idle().await?;

                let zero = VectorXf::zero_with_shape(num_axes, 1);
                self.motion_controller.set_position(zero.clone()).await?;

                println!("Zeroed!");

                let mut current_pos = zero.clone();

                // Lift in Z
                current_pos[2] = 10.0;
                self.motion_controller.move_to(current_pos.clone(), 10.0).await?;
                self.motion_controller.wait_until_idle().await?;


                for axis_i in 0..2 {
                    let xy_endstops = vec!["stepper1_stall".to_string(), "stepper2_stall".to_string()];
                    self.endstop_controller.start(&xy_endstops, &xy_endstops).await?;

                    {
                        current_pos[axis_i] = -200.0;
                        self.motion_controller.move_to(current_pos.clone(), feed_rate).await?;
                        println!("Ramping min {}!", axis_i);
                    }

                    self.motion_controller.wait_until_idle().await?;

                    // tODO: Must verify we actually hit the expected endstops (and reset to using all normal endstops.)

                    println!("Ramped min {}!", axis_i);

                    current_pos[axis_i] = 0.0;
                    self.motion_controller.set_position(current_pos.clone()).await?;

                    println!("Backing off");
                    {
                        current_pos[axis_i] = 20.0;
                        self.motion_controller.move_to(current_pos.clone(), feed_rate).await?;
                    }

                    self.motion_controller.wait_until_idle().await?;
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

                let dev = self.devices.get_peripherals_device("toolhead").await?;

                let z_endstops = vec!["z_probe".to_string()];
                self.endstop_controller.start(&z_endstops, &z_endstops).await?;

                current_pos[2] = -200.0;
                self.motion_controller.move_to(current_pos.clone(), 5.0).await?;

                self.motion_controller.wait_until_idle().await?;

                // TODO: Eventually rely on better timing data about hits to improve this.
                current_pos[2] = -0.1;
                self.motion_controller.set_position(current_pos.clone()).await?;

                current_pos[2] = 10.0;
                self.motion_controller.move_to(current_pos.clone(), 10.0).await?;
            }
        }

        Ok(())
    }
}
