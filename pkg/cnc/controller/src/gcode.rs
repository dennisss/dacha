use common::errors::*;
use cnc_controller_proto::cnc::Command;


pub struct CommandConverter {
    last_pos: Vec<gcode::Decimal>,
    last_feed_rate: f32
}

impl CommandConverter {

    pub fn new() -> Self {
        Self {
            last_pos: vec![0.into(), 0.into(), 0.into(), 0.into()],
            last_feed_rate: 0.0,
        }
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
            gcode::Command::SetToAbsoluteMode(v) => {
                // println!("ABS: {:?}", v);
            }
            gcode::Command::SetExtruderToRelativeMode(v) => {


                // println!("{:?}", v);
            }
            gcode::Command::FanOn(_) => {

            }

            // LinearMove(LinearMove { inner: Move { x: Some(51.312), y: Some(57.403), z: None, e: Some(0.29417), feed_rate: None } })
            gcode::Command::LinearMove(cmd) => {

                // TODO: Use deref and get rid of the inners.
                if let Some(v) = cmd.inner.x {
                    self.last_pos[0] = v;
                }
                if let Some(v) = cmd.inner.y {
                    self.last_pos[1] = v;
                }
                if let Some(v) = cmd.inner.z {
                    self.last_pos[2] = v;
                }
                if let Some(v) = cmd.inner.e {
                    // TODO: Only use '+=' if in relative extruder mode.
                    self.last_pos[3] += v;
                }
                if let Some(v) = cmd.inner.feed_rate {
                    // The value is in mm/min
                    self.last_feed_rate = (v.to_f32() / 60.0);
                }

                let mut cmd = Command::default();
                let move_to = cmd.move_to_mut();
                move_to.set_x(self.last_pos[0].to_f32());
                move_to.set_y(self.last_pos[1].to_f32());
                move_to.set_z(self.last_pos[2].to_f32());
                move_to.set_e(self.last_pos[3].to_f32());
                move_to.set_feed_rate(self.last_feed_rate);

                out.push(cmd);
            }
            gcode::Command::FanOff(_) => {

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

                self.last_pos[3] = 0.into();

                let mut cmd = Command::default();
                let p = cmd.set_position_mut();
                p.set_x(self.last_pos[0].to_f32());
                p.set_y(self.last_pos[1].to_f32());
                p.set_z(self.last_pos[2].to_f32());
                p.set_e(self.last_pos[3].to_f32());

                out.push(cmd);
            }
            c @ _ => {
                eprintln!("Uknown: {:?}", c);
            }

        }

        Ok(())
    }

}