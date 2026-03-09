use common::errors::*;
use cnc_controller_proto::cnc::Command;
use math::matrix::{VectorXd, MatrixXd};
use cnc_controller::proto_utils::*;

use crate::leveling::*;


pub struct CommandConverter {
    absolute_mode_set: bool,
    extruder_relative_mode: bool,

    last_machine_position: VectorXd,
    last_position: Vec<gcode::Decimal>,
    last_feed_rate: f64,
    leveler: Option<ZGridFadeLeveler>,
    skew: Option<MatrixXd>,
}

impl CommandConverter {

    pub fn new(last_machine_position: VectorXd) -> Self {
        // We assume the last position perfectly translates to the 
        // last machine position after any skew/leveling is applied.
        // This has the main issue of making the first move in the file
        // to be awkward if it doesn't explicitly specify all axes. 
        let mut last_position = vec![];
        for i in 0..last_machine_position.len() {
            last_position.push(last_machine_position[i].into());
        }

        Self {
            last_machine_position,
            last_position,
            last_feed_rate: 0.0,
            absolute_mode_set: false,
            extruder_relative_mode: false,
            leveler: None,
            skew: None,
        }
    }

    pub fn set_leveler(&mut self, leveler: Option<ZGridFadeLeveler>) {
        self.leveler = leveler;
    }

    pub fn set_skew(&mut self, skew: Option<MatrixXd>) {

        let mat = match skew {
            Some(v) => v,
            None => {
                self.skew = None;
                return;
            }
        };

        let mut extended = MatrixXd::identity_with_shape(
            self.last_machine_position.len(), self.last_machine_position.len());

        for i in 0..mat.rows() {
            for j in 0..mat.cols() {
                extended[(i, j)] = mat[(i, j)];
            }
        }

        self.skew = Some(extended);
    }

    fn decimal_to_vector(v: &[gcode::Decimal]) -> VectorXd {
        let mut values = v.iter().map(|v| v.to_f64()).collect::<Vec<_>>();
        VectorXd::from_slice_with_shape(values.len(), 1, &values)
    }

    pub fn next(&mut self, command: &gcode::Command, out: &mut Vec<Command>) -> Result<()> {
        match command {
            gcode::Command::SetBedTemperatureAndWaitCommand(_) => {

            }
            gcode::Command::SetExtruderTemperature(_) => {

            }
            gcode::Command::SetExtruderTemperatureAndWait(_) => {

            }
            gcode::Command::SetUnitsToMillimeters(_) => {

            }
            gcode::Command::SetToRelativeMode(_) => {
                self.absolute_mode_set = false;
            }
            gcode::Command::SetToAbsoluteMode(_) => {
                self.absolute_mode_set = true;
            }
            gcode::Command::SetExtruderToRelativeMode(_) => {
                self.extruder_relative_mode = true;
            }
            gcode::Command::SetExtruderToAbsoluteMode(_) => {
                self.extruder_relative_mode = false;
            }
            gcode::Command::FanOn(cmd) => {
                // TODO: Dedup this logic a bit.
                let speed = cmd
                    .speed
                    .ok_or_else(|| err_msg("M106 requires S parameter"))?
                    .to_f32();

                if speed < 0.0 || speed > 255.0 {
                    return Err(err_msg("Invalid fan speed"));
                }

                let mut cmd = Command::default();
                cmd.set_fan_speed_mut().set_speed(speed / 255.0);
                out.push(cmd);
            }
            gcode::Command::FanOff(_) => {
                let mut cmd = Command::default();
                cmd.set_fan_speed_mut().set_speed(0.0);
                out.push(cmd);
            }
            // LinearMove(LinearMove { inner: Move { x: Some(51.312), y: Some(57.403), z: None, e: Some(0.29417), feed_rate: None } })
            gcode::Command::LinearMove(cmd) => {

                if !self.absolute_mode_set {
                    return Err(err_msg("Only absolute moves supported"));
                }

                // TODO: Want to use an already transformed position.
                let start_machine_position = &self.last_machine_position;

                // TODO: Use deref and get rid of the inners.
                if let Some(v) = cmd.inner.x {
                    self.last_position[0] = v;
                }
                if let Some(v) = cmd.inner.y {
                    self.last_position[1] = v;
                }
                if let Some(v) = cmd.inner.z {
                    self.last_position[2] = v;
                }
                if let Some(v) = cmd.inner.e {
                    if self.extruder_relative_mode {
                        self.last_position[3] += v;
                    } else {
                        self.last_position[3] = v;
                    }
                }
                if let Some(v) = cmd.inner.feed_rate {
                    // The value is in mm/min
                    self.last_feed_rate = (v.to_f64() / 60.0);
                }

                let end_position = Self::decimal_to_vector(&self.last_position);

                let end_machine_position = {
                    if let Some(skew) = &self.skew {
                        skew * end_position
                    } else {
                        end_position
                    }
                };

                let machine_positions = {
                    if let Some(leveler) = &self.leveler {
                        leveler.rewrite_move(start_machine_position, &end_machine_position, false)
                    } else {
                        vec![end_machine_position]
                    }
                };


                // TODO: For this to be efficient, we need to make sure that step motions
                // are compressed across multiple moves in the StepMotionGenerator

                for pos in machine_positions {
                    let mut cmd = Command::default();
                    let move_to = cmd.move_to_mut();
                    move_to.set_position(pos.to_proto());
                    move_to.options_mut().set_feed_rate(self.last_feed_rate);
                    out.push(cmd);

                    self.last_machine_position = pos;
                }
            }

            gcode::Command::SetDefaultAcceleration(_) => {

            }
            gcode::Command::SetPosition(v) => {

                // println!("Set Pos: {:?}", v);
                // TODO: Maybe implement this at some point.
                // https://forum.prusa3d.com/forum/prusaslicer/add-g92-e0/
                // But we are doing most additions in decimal exact point so this is probably ok?

                // TODO: This is used for setting extruder position to zero.

                // TODO: Internally on the server have this only touch the 'E' axis.

                // TODO: Must apply machine transform.
                // (currently doesn't matter though if we only support E set_position).

                self.last_position[3] = 0.into();
                self.last_machine_position[3] = 0.0;

                let mut cmd = Command::default();
                let p = cmd.set_position_mut();
                p.set_position(self.last_machine_position.to_proto());
                out.push(cmd);
            }
            c @ _ => {
                eprintln!("Uknown: {:?}", c);
            }

        }

        Ok(())
    }

}