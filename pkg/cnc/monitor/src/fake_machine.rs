use std::time::Instant;
use std::{sync::Arc, time::Duration};

use base_error::*;
use common::io::{Readable, Writeable};
use executor::bundle::TaskResultBundle;
use executor::sync::AsyncMutex;
use executor::{lock, lock_async};
use math::matrix::Vector3f;

use crate::serial_receiver_buffer::SerialReceiverBuffer;

const AMBIENT_TEMPERATURE: f32 = 25.0;
const MAX_TEMPERATURE: f32 = 400.0;
const TEMP_CHANGE_PER_SECOND: f32 = 4.0;
const PART_COOLING_FAN_MAX_RPM: f32 = 4000.0;

/// Fake machine implementation which can be interacted with over an in-process
/// 'serial connection'.
///
/// - Marlin style gcode support and responses.
/// - 3 axes (XYZ)
/// - 1 tool with: 1 extruder axis, 1 part cooling fan, 1 extruder fan.
/// - 1 heated bed.
/// - Assumes infinite acceleration and moves at exactly the requested feedrate.
/// - The hotend can heat or cool by 10 deg C per second.
/// - The serial connection will not return 'ok'/'error' until each command is
///   completely executed.
pub struct FakeMachine {
    shared: Arc<Shared>,
}

struct Shared {
    state: AsyncMutex<State>,
    serial_writer: AsyncMutex<Box<dyn Writeable>>,
}

struct State {
    /// Current X, Y, Z axis positions in mm units.
    position: Vector3f,
    position_target: Vector3f,
    position_absolute_mode: bool,

    /// Current feed rate in mm/min units.
    feed_rate: f32,

    extruder_position: f32,
    extruder_position_target: f32,
    extruder_absolute_mode: bool,
    extruder_temperature: f32,
    extruder_target_temperature: f32,

    /// From 0-1.
    part_cooling_fan_value: f32,

    heatbed_temperature: f32,
    heatbed_target_temperature: f32,

    auto_report_interval: Duration,
}

impl FakeMachine {
    pub async fn create() -> Result<(Box<dyn Readable>, Box<dyn Writeable>)> {
        // Send commands end.
        let (client_writer, server_reader) = executor::pipe::pipe();
        // Return responses end
        let (server_writer, client_reader) = executor::pipe::pipe();

        let shared = Arc::new(Shared {
            state: AsyncMutex::new(State {
                position: Vector3f::zero(),
                position_target: Vector3f::zero(),
                position_absolute_mode: true,
                feed_rate: 60.0,
                extruder_position: 0.0,
                extruder_position_target: 0.0,
                extruder_absolute_mode: true,
                extruder_temperature: AMBIENT_TEMPERATURE,
                extruder_target_temperature: 0.0,
                part_cooling_fan_value: 0.0,
                heatbed_temperature: AMBIENT_TEMPERATURE,
                heatbed_target_temperature: 0.0,
                auto_report_interval: Duration::ZERO,
            }),
            serial_writer: AsyncMutex::new(Box::new(server_writer)),
        });

        executor::spawn(Self::main_thread(shared, Box::new(server_reader)));

        Ok((Box::new(client_reader), Box::new(client_writer)))
    }

    async fn main_thread(shared: Arc<Shared>, serial_reader: Box<dyn Readable>) {
        let mut bundle = TaskResultBundle::new();
        bundle.add(
            "serial_thread",
            Self::serial_thread(shared.clone(), serial_reader),
        );
        bundle.add("control_loop", Self::control_loop(shared.clone()));
        bundle.add("auto_report_loop", Self::auto_report_loop(shared.clone()));

        if let Err(e) = bundle.join().await {
            eprintln!("FakeMachine failed: {}", e);
        }
    }

    async fn serial_thread(
        shared: Arc<Shared>,
        mut serial_reader: Box<dyn Readable>,
    ) -> Result<()> {
        let mut chunk_buffer = vec![0u8; 256];
        let mut parser = gcode::ProgramParser::default();
        let mut elements = vec![];

        // TODO: Throttle this loop.
        loop {
            let mut remaining = {
                // TODO: Catch closed errors and return ok.
                let n = serial_reader.read(&mut chunk_buffer).await?;
                if n == 0 {
                    return Err(err_msg("Hit end of serial?"));
                }

                &chunk_buffer[0..n]
            };

            while !remaining.is_empty() {
                let n = parser.parse_line(remaining, false, &mut elements);
                remaining = &remaining[n..];

                if let Some(gcode::ProgramElement::EndOfLine) = elements.last() {
                    if let Err(e) = Self::process_line(&shared, &mut elements).await {
                        eprintln!("FakeMachine Processing Error: {}", e);

                        lock_async!(writer <= shared.serial_writer.lock().await?, {
                            writer.write_all(b"error\n").await
                        })?;
                        continue;
                    }

                    lock_async!(writer <= shared.serial_writer.lock().await?, {
                        writer.write_all(b"ok\n").await
                    })?;
                }
            }
        }
    }

    /// Applies the effect of an incoming line to the state of the machine.
    ///
    /// Depending on the return result of this, we will either send an 'ok' or
    /// 'error' back to the host.
    async fn process_line(shared: &Shared, line: &mut Vec<gcode::ProgramElement>) -> Result<()> {
        // TODO: Need to process commands in a consistent order. (e.g. apply coordinate
        // system changes before doing moves)
        for el in line.drain(..) {
            let cmd = match el {
                gcode::ProgramElement::Command(v) => v,
                gcode::ProgramElement::Error(e) => {
                    return Err(e);
                }
                _ => continue,
            };

            match cmd {
                gcode::Command::RapidMove(cmd) => {
                    Self::process_move(shared, &cmd.inner).await?;
                }
                gcode::Command::LinearMove(cmd) => {
                    Self::process_move(shared, &cmd.inner).await?;
                }

                gcode::Command::Dwell(_) | gcode::Command::WaitForCurrentMovesToFinish(_) => {
                    // Machine is idle after each command, so should still be
                    // idle here.
                }

                gcode::Command::SetUnitsToInches(_) => {
                    return Err(err_msg("Inches are not supported"));
                }
                gcode::Command::SetUnitsToMillimeters(_) => {}

                gcode::Command::MoveToOriginHome(_) => {
                    // Ignore params.
                }

                gcode::Command::G80(_) => {
                    // Prusa specific mesh based z-probe
                }

                gcode::Command::SetToAbsoluteMode(_) => {
                    lock!(state <= shared.state.lock().await?, {
                        state.position_absolute_mode = true;
                    });
                }

                gcode::Command::SetToRelativeMode(_) => {
                    lock!(state <= shared.state.lock().await?, {
                        state.position_absolute_mode = false;
                    });
                }

                gcode::Command::SetPosition(cmd) => {
                    lock!(state <= shared.state.lock().await?, {
                        if state.position != state.position_target
                            || state.extruder_position_target != state.extruder_position
                        {
                            return Err(err_msg("Set position not allowed while moving"));
                        }

                        let mut some_set = false;

                        let mut new_pos = state.position.clone();
                        for (i, value) in [cmd.x, cmd.y, cmd.z].into_iter().enumerate() {
                            if let Some(v) = value {
                                new_pos[i] = v.to_f32();
                                some_set = true;
                            }
                        }

                        state.position = new_pos.clone();
                        state.position_target = new_pos;

                        if let Some(v) = cmd.e {
                            state.extruder_position = v.to_f32();
                            state.extruder_position_target = v.to_f32();
                            some_set = true;
                        }

                        if !some_set {
                            return Err(err_msg(
                                "Ambigious behavior when G92 is called without any params",
                            ));
                        }

                        Ok(())
                    })?;
                }

                gcode::Command::SetExtruderToAbsoluteMode(_) => {
                    lock!(state <= shared.state.lock().await?, {
                        state.extruder_absolute_mode = true;
                    });
                }
                gcode::Command::SetExtruderToRelativeMode(_) => {
                    lock!(state <= shared.state.lock().await?, {
                        state.extruder_absolute_mode = false;
                    });
                }
                gcode::Command::SetBuildPercentage(_) => {}
                gcode::Command::StopMotors(_) => {}
                gcode::Command::SetExtruderTemperature(cmd) => {
                    Self::process_set_heater(shared, &cmd.inner, false, true).await?;
                }
                gcode::Command::SetExtruderTemperatureAndWait(cmd) => {
                    Self::process_set_heater(shared, &cmd.inner, true, true).await?;
                }
                gcode::Command::GetCurrentPosition(_)
                | gcode::Command::GetExtruderTemperature(_)
                | gcode::Command::GetTachometerValue(_) => {
                    let report = lock!(state <= shared.state.lock().await?, {
                        Self::generate_state_report(&state)
                    });

                    lock_async!(writer <= shared.serial_writer.lock().await?, {
                        writer.write_all(report.as_bytes()).await
                    })?;
                }
                gcode::Command::FanOn(cmd) => {
                    let speed = cmd
                        .speed
                        .ok_or_else(|| err_msg("M106 requires S parameter"))?
                        .to_f32();

                    if speed < 0.0 || speed > 255.0 {
                        return Err(err_msg("Invalid fan speed"));
                    }

                    lock!(state <= shared.state.lock().await?, {
                        state.part_cooling_fan_value = speed / 255.0;
                    });
                }
                gcode::Command::FanOff(_) => {
                    lock!(state <= shared.state.lock().await?, {
                        state.part_cooling_fan_value = 0.0;
                    });
                }
                gcode::Command::SetDebugLevel(_) => {}
                gcode::Command::PrintFirmwareCapabilities(_) => {
                    lock_async!(writer <= shared.serial_writer.lock().await?, {
                        writer.write_all(b"Cap:AUTOREPORT_TEMP:1\n").await?;
                        writer.write_all(b"Cap:AUTOREPORT_FANS:1\n").await?;
                        writer.write_all(b"Cap:AUTOREPORT_POSITION:1\n").await
                    })?;
                }
                gcode::Command::SetBedTemperature(cmd) => {
                    Self::process_set_heater(shared, &cmd.inner, false, false).await?;
                }

                gcode::Command::SetBedTemperatureAndWaitCommand(cmd) => {
                    Self::process_set_heater(shared, &cmd.inner, true, false).await?;
                }

                gcode::Command::SetupAutoReport(cmd) => {
                    // TODO: Interpret the flags.

                    lock!(state <= shared.state.lock().await?, {
                        state.auto_report_interval =
                            Duration::from_secs_f32(cmd.interval_secs as f32);
                    });
                }

                gcode::Command::SetMaxAcceleration(_)
                | gcode::Command::SetMaxFeedRate(_)
                | gcode::Command::SetDefaultAcceleration(_)
                | gcode::Command::AdvancedSettings(_)
                | gcode::Command::ExtruderPressureAdvance(_)
                | gcode::Command::NozzleDiameter(_)
                | gcode::Command::SetLinearAdvanceScalingFactors(_)
                | gcode::Command::SetMotorCurrent(_)
                | gcode::Command::SetExtrudeFactorOverride(_) => {}

                _ => {
                    return Err(format_err!("Unsupported command: {:?}", cmd));
                }
            }
        }

        Ok(())
    }

    async fn process_set_heater(
        shared: &Shared,
        cmd: &gcode::SetHeaterTemperature,
        wait: bool,
        is_extruder: bool,
    ) -> Result<()> {
        let mut temp = None;

        if let Some(t) = cmd.min_temperature {
            temp = Some(t);
        }

        if let Some(t) = cmd.target_temperature {
            if let Some(t2) = temp {
                if t != t2 {
                    return Err(err_msg("Inconsistent temperature requests"));
                }
            }

            if !wait {
                return Err(err_msg("Target temperature only allowed in wait mode"));
            }

            temp = Some(t);
        }

        let temp = match temp {
            Some(v) => v.to_f32(),
            None => return Err(err_msg("No temperature in wait request")),
        };

        if let Some(t) = cmd.tool {
            if t != 0 || !is_extruder {
                return Err(err_msg("Invalid tool parameter"));
            }
        }

        lock!(state <= shared.state.lock().await?, {
            if is_extruder {
                state.extruder_target_temperature = temp;
            } else {
                state.heatbed_target_temperature = temp;
            }
        });

        if !wait {
            return Ok(());
        }

        // TODO: Make a helper function for this.
        loop {
            let done = lock!(state <= shared.state.lock().await?, {
                if is_extruder {
                    state.extruder_temperature == state.extruder_target_temperature
                } else {
                    state.heatbed_temperature == state.heatbed_target_temperature
                }
            });

            if done {
                break;
            }

            executor::sleep(Duration::from_millis(10)).await?;
        }

        Ok(())
    }

    async fn process_move(shared: &Shared, m: &gcode::Move) -> Result<()> {
        // TODO: Must reject moves with more than then justthe XYZE axes.

        lock!(state <= shared.state.lock().await?, {
            let mut new_pos = state.position.clone();
            for (i, value) in [m.x, m.y, m.z].into_iter().enumerate() {
                if let Some(v) = value {
                    if state.position_absolute_mode {
                        new_pos[i] = v.to_f32();
                    } else {
                        new_pos[i] += v.to_f32();
                    }
                }
            }
            state.position_target = new_pos;

            if let Some(v) = m.e {
                if state.extruder_absolute_mode {
                    state.extruder_position_target = v.to_f32();
                } else {
                    state.extruder_position_target += v.to_f32();
                }
            }

            if let Some(v) = m.feed_rate {
                state.feed_rate = v.to_f32();
            }

            Ok::<_, Error>(())
        })?;

        // Wait for motion to complete.
        loop {
            let done = lock!(state <= shared.state.lock().await?, {
                state.position == state.position_target
                    && state.extruder_position == state.extruder_position_target
            });

            if done {
                break;
            }

            executor::sleep(Duration::from_millis(10)).await?;
        }

        Ok(())
    }

    /// Simulates physics timesteps to update the current physical state.
    async fn control_loop(shared: Arc<Shared>) -> Result<()> {
        let mut last_update = Instant::now();
        loop {
            let now = Instant::now();
            let dt = now.duration_since(last_update).as_secs_f32();

            // TODO: Need to interpolate based only on the XYZ and have E axis follow that.

            lock!(state <= shared.state.lock().await?, {
                // Update position.
                // TODO: Need to support extruder only moves.
                {
                    let mm_per_s = state.feed_rate / 60.0;

                    // Max mm that we can move in this time step.
                    let mm_delta = mm_per_s * dt;

                    let remaining_move = &state.position_target - &state.position;

                    if remaining_move.norm() <= mm_delta {
                        state.position = state.position_target.clone();
                        state.extruder_position = state.extruder_position_target;
                    } else {
                        state.position += remaining_move.normalized() * mm_delta;
                        // TODO: Also update the extruder position.
                    }
                }

                state.extruder_temperature = Self::next_temperature(
                    state.extruder_temperature,
                    state.extruder_target_temperature,
                    dt,
                );

                state.heatbed_temperature = Self::next_temperature(
                    state.heatbed_temperature,
                    state.heatbed_target_temperature,
                    dt,
                );
            });

            last_update = now;

            executor::sleep(Duration::from_millis(10)).await?;
        }
    }

    fn next_temperature(current_temp: f32, target_temp: f32, dt: f32) -> f32 {
        // TODO: Add some smoothing when we approach the target.
        let mut t =
            current_temp + (target_temp - current_temp).signum() * TEMP_CHANGE_PER_SECOND * dt;
        t = f32::min(t, MAX_TEMPERATURE);
        t = f32::max(t, AMBIENT_TEMPERATURE);

        if (t - target_temp).abs() < 0.01 {
            t = target_temp;
        }

        t
    }

    async fn auto_report_loop(shared: Arc<Shared>) -> Result<()> {
        let mut last_report_time = Instant::now();
        loop {
            let now = Instant::now();

            let report = lock!(state <= shared.state.lock().await?, {
                if state.auto_report_interval.is_zero() {
                    None
                } else {
                    if now - last_report_time >= state.auto_report_interval {
                        Some(Self::generate_state_report(&state))
                    } else {
                        None
                    }
                }
            });

            if let Some(report) = report {
                last_report_time = now;

                lock_async!(writer <= shared.serial_writer.lock().await?, {
                    writer.write_all(report.as_bytes()).await
                })?;
            }

            executor::sleep(Duration::from_millis(200)).await?;
        }
    }

    fn generate_state_report(state: &State) -> String {
        let mut out = String::new();

        out.push_str(&format!(
            "T:{etemp:.01} /{etarget:.01} B:{btemp:.01} /{btarget:.01} T0:{etemp:.01} /{etarget:.01} @:0 B@:0 P:0.0 A:31.2\n",
            etemp = state.extruder_temperature, etarget = state.extruder_target_temperature,
            btemp = state.heatbed_temperature, btarget = state.heatbed_target_temperature
        ));

        out.push_str(&format!(
            "X:{:.02} Y:{:.02} Z:{:.02} E:{:.02} Count X: 0.00 Y:0.00 Z:0.00 E:0.00\n",
            state.position[0], state.position[1], state.position[2], state.extruder_position
        ));

        // TODO: Format with simulated values.
        out.push_str(&format!("E0:0 RPM PRN1:0 RPM E0@:0 PRN1@:0\n"));

        out
    }
}
