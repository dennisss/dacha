use std::sync::Arc;
use std::time::Duration;
use std::f64::consts::PI;

use common::errors::*;
use math::matrix::VectorXd;
use math::matrix::MatrixXd;
use math::vecxd;
use executor_multitask::RootResource;
use cluster_client::ClusterMetaClient;
use cnc_controller_proto::cnc::*;
use cnc_controller::motion_controller_sim::MotionControllerSimulator;
use cnc_controller::motion_controller::MotionController;
use cnc_controller::config::ControllerConfigRegistry;
use cnc_controller::proto_utils::*;
use file::LocalPathBuf;


use crate::leveling::*;
use crate::gcode::*;
use crate::remote::*;

#[derive(Args)]
pub struct ExecuteCommand {
    // #[arg(positional)]
    proto: Option<String>,

    gcode_file: Option<LocalPathBuf>,

    z_leveler: Option<LocalPathBuf>,

    skew: Option<LocalPathBuf>,

    #[arg(default = false)]
    circle: bool,

    extrude: Option<f64>,

    rel_x: Option<f64>,
    rel_y: Option<f64>,
    rel_z: Option<f64>,

    // TODO: Ideally make everything in the cnc_tools binary simulatable.
    #[arg(default = false)]
    simulate: bool
}

impl ExecuteCommand {
    pub async fn run(self) -> Result<()> {
        // TODO: If we know where we have printed, we can do collision blocking to prevent moves that intersect with that.

        let mut machine = None;
        if !self.simulate {
            machine = Some(RemoteMachineController::create().await?);
        }


        if self.rel_x.is_some() || self.rel_y.is_some() || self.rel_z.is_some() || self.extrude.is_some() {

            let machine = machine.as_mut().unwrap();

            let mut pos = machine.last_position().await?;
            if let Some(v) = self.rel_x {
                pos[0] += v;
            }
            if let Some(v) = self.rel_y {
                pos[1] += v;
            }
            if let Some(v) = self.rel_z {
                pos[2] += v;
            }

            if let Some(v) = self.extrude {
                pos[3] += v;
            }

            machine.move_to(&pos, 10.0).await?;
            machine.wait_until_idle().await?;
        }

        if self.circle {

            let machine = machine.as_mut().unwrap();

            let radius = 20.0;
            let num_parts = 16;

            let mut request = ExecuteRequest::default();

            {
                let cmd = request.new_commands();
                cmd.configure_mut().set_max_junction_deviation(0.2);
            }

            for i in 0..(num_parts + 1) {
                let angle = (i as f64) * (2.0 * PI) / (num_parts as f64);

                let x = 60.0 + radius * angle.cos();
                let y = 60.0 + radius * angle.sin();

                println!("{},{}", x, y);

                let cmd = request.new_commands();
                let m = cmd.move_to_mut();

                m.set_x(x);
                m.set_y(y);
                m.set_z(10.0);
                m.options_mut().set_feed_rate(20.0);
            }

            machine.execute(&request).await?;

            return Ok(())
        }

        if let Some(path) = self.gcode_file {

            let last_pos = {
                if self.simulate {
                    vecxd!(0.0, 0.0, 0.0, 0.0)
                } else {
                    machine.as_mut().unwrap().last_position().await?
                }
            };

            let gcode_cmds = Self::get_all_gcode_commands(&path).await?;

            let mut converter = CommandConverter::new(last_pos.clone());

            if let Some(path) = self.z_leveler {
                let mut proto = ZGridFadeLevelerProto::default();
                let data = file::read_to_string(&path).await?;
                protobuf::text::parse_text_proto(&data, &mut proto)?;
                converter.set_leveler(Some(
                    ZGridFadeLeveler::from_proto(&proto)
                ));
            }

            if let Some(path) = self.skew {
                let mut proto = MatrixProto::default();
                let data = file::read_to_string(&path).await?;
                protobuf::text::parse_text_proto(&data, &mut proto)?;
                converter.set_skew(Some(MatrixXd::from_proto(&proto)));
            }

            let mut cmds = vec![];

            for gcode_cmd in gcode_cmds {
                let mut out = vec![];
                converter.next(&gcode_cmd, &mut out)?;
                cmds.extend(out.into_iter());
            }

            if self.simulate {

                Self::simulate_execution(&cmds, last_pos.clone()).await?;

            } else {
                let machine = machine.as_mut().unwrap();

                // TODO: Batch this.
                for chunk in cmds.chunks(8) {
                    let mut request = ExecuteRequest::default();
                    for c in chunk {
                        request.add_commands(c.clone());
                    }
                    // TODO: This doesn't seem to consistently error out if we are in an alarm mode.
                    machine.execute(&request).await?;
                }

                // Lift up after the print is done.
                let mut last_pos = machine.last_position().await?;
                last_pos[2] += 10.0;
                machine.move_to(&last_pos, 10.0).await?;
            }


            // TODO: Need final commands and raising and going to (5, 5, 0) or something like that.
        }

        if let Some(proto) = &self.proto {
            let machine = machine.as_mut().unwrap();

            let mut request = ExecuteRequest::default();
            protobuf::text::parse_text_proto(&proto, &mut request)?;
            println!("{:?}", machine.execute(&request).await?);
        }

        Ok(())
    }

    async fn simulate_execution(commands: &[Command], start_position: VectorXd) -> Result<()> {
        let config_name = "voron0";

        let mut config_registry = ControllerConfigRegistry::defaults().await?;
        let mut config = config_registry.remove(config_name)
            .ok_or_else(|| format_err!("No config named: {}", config_name))?;

        MotionController::adjust_config(config.motion_controller_mut())?;

        let motion_config = Arc::new(config.motion_controller().clone());

        let mut sim = MotionControllerSimulator::new(motion_config.clone(), start_position);
        sim.run(commands)?;

        Ok(())
    }

    async fn get_all_gcode_commands(path: &file::LocalPath) -> Result<Vec<gcode::Command>> {
        let data = file::read(&path).await?;
        parse_gcode_string(&data)
    }
}


