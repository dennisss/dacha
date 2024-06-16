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
const TEMP_CHANGE_PER_SECOND: f32 = 10.0;
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
        let (client_writer, server_reader) = common::pipe::pipe();
        // Return responses end
        let (server_writer, client_reader) = common::pipe::pipe();

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
        let mut buf = SerialReceiverBuffer::default();
        let mut chunk_buffer = vec![0u8; 256];

        let mut line_offset = buf.last_line_offset().await?;

        // TODO: Throttle this loop.
        loop {
            {
                // TODO: Catch closed errors and return ok.
                let n = serial_reader.read(&mut chunk_buffer).await?;
                if n == 0 {
                    return Err(err_msg("Hit end of serial?"));
                }

                buf.append(&chunk_buffer[0..n], Instant::now()).await?;
            }

            let last_line_offset = buf.last_line_offset().await?;

            while line_offset < last_line_offset {
                let line_entry = buf.get_line(line_offset).await?;
                line_offset += 1;

                let line = match Self::decode_line(&line_entry.data) {
                    Ok(Some(v)) => v,
                    Ok(None) => {
                        lock_async!(writer <= shared.serial_writer.lock().await?, {
                            writer.write_all(b"ok\n").await
                        })?;
                        continue;
                    }
                    Err(e) => {
                        lock_async!(writer <= shared.serial_writer.lock().await?, {
                            writer.write_all(b"error: Invalid line received\n").await
                        })?;
                        continue;
                    }
                };

                if let Err(e) = Self::process_line(&shared, &line).await {
                    eprintln!("FakeMachine Processing Error: {}", e);

                    lock_async!(writer <= shared.serial_writer.lock().await?, {
                        writer.write_all(b"error\n").await
                    })?;
                } else {
                    lock_async!(writer <= shared.serial_writer.lock().await?, {
                        writer.write_all(b"ok\n").await
                    })?;
                }
            }
        }
    }

    fn decode_line(line: &[u8]) -> Result<Option<gcode::Line>> {
        let mut builder = gcode::LineBuilder::new();

        let mut parser = gcode::Parser::new();
        let mut iter = parser.iter(line, true);
        while let Some(e) = iter.next() {
            match e {
                gcode::Event::ParseError(_) => {
                    return Err(err_msg("ParseError in line"));
                }
                gcode::Event::Word(w) => {
                    builder.add_word(w)?;
                }
                _ => {}
            }
        }

        Ok(builder.finish())
    }

    /// Applies the effect of an incoming line to the state of the machine.
    ///
    /// Depending on the return result of this, we will either send an 'ok' or
    /// 'error' back to the host.
    async fn process_line(shared: &Shared, line: &gcode::Line) -> Result<()> {
        let cmd = line.command().to_string();
        let mut params = line.params().clone();

        match cmd.as_str() {
            "G0" | "G1" => {
                lock!(state <= shared.state.lock().await?, {
                    let mut new_pos = state.position.clone();
                    for (i, axis) in ['X', 'Y', 'Z'].into_iter().enumerate() {
                        if let Some(v) = params.remove(axis) {
                            if state.position_absolute_mode {
                                new_pos[i] = v.to_f32()?;
                            } else {
                                new_pos[i] += v.to_f32()?;
                            }
                        }
                    }
                    state.position_target = new_pos;

                    if let Some(v) = params.remove(&'E') {
                        if state.extruder_absolute_mode {
                            state.extruder_position_target = v.to_f32()?;
                        } else {
                            state.extruder_position_target += v.to_f32()?;
                        }
                    }

                    if let Some(v) = params.remove(&'F') {
                        state.feed_rate = v.to_f32()?;
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
            }
            // Dwell
            "G4" => {
                // Machine is idle after each command, so should still be idle
                // here.
            }

            // Set to inches
            "G20" => {
                return Err(err_msg("Inches are not supported"));
            }
            // Set to mm units
            "G21" => {}

            // Move to origin (home)
            "G28" => {
                params.clear();
            }

            // Prusa sopecific mesh based z-probe
            "G80" => {
                params.clear();
            }

            // Set to absolute positioning
            "G90" => {
                lock!(state <= shared.state.lock().await?, {
                    state.position_absolute_mode = true;
                });
            }
            // Set to relative positioning
            "G91" => {
                lock!(state <= shared.state.lock().await?, {
                    state.position_absolute_mode = false;
                });
            }

            // Set position
            "G92" => {
                if params.is_empty() {
                    return Err(err_msg(
                        "Ambigious behavior when G92 is called without any params",
                    ));
                }

                lock!(state <= shared.state.lock().await?, {
                    if state.position != state.position_target
                        || state.extruder_position_target != state.extruder_position
                    {
                        return Err(err_msg("Set position not allowed while moving"));
                    }

                    let mut new_pos = state.position.clone();
                    for (i, axis) in ['X', 'Y', 'Z'].into_iter().enumerate() {
                        if let Some(v) = params.remove(axis) {
                            new_pos[i] = v.to_f32()?;
                        }
                    }

                    state.position = new_pos.clone();
                    state.position_target = new_pos;

                    if let Some(v) = params.remove(&'E') {
                        state.extruder_position = v.to_f32()?;
                        state.extruder_position_target = v.to_f32()?;
                    }

                    Ok(())
                })?;
            }

            // Set extruder to absolute mode.
            "M82" => {
                lock!(state <= shared.state.lock().await?, {
                    state.extruder_absolute_mode = true;
                });
            }

            // Set extruder to relative mode.
            "M83" => {
                lock!(state <= shared.state.lock().await?, {
                    state.extruder_absolute_mode = false;
                });
            }

            // Set/get build percentage.
            "M73" => {
                params.clear();
            }

            // Stop motors
            "M84" => {}

            // Set extruder temperature
            "M104" => {
                let temp = params
                    .remove(&'S')
                    .ok_or_else(|| err_msg("M109 requires S parameter"))?
                    .to_f32()?;

                lock!(state <= shared.state.lock().await?, {
                    state.extruder_target_temperature = temp;
                });
            }

            /*
            M114 - get position
            M105 - get temp
            M123 - get tachometer values (prusa/marlin)
            */
            "M105" | "M114" | "M123" => {
                let report = lock!(state <= shared.state.lock().await?, {
                    Self::generate_state_report(&state)
                });

                lock_async!(writer <= shared.serial_writer.lock().await?, {
                    writer.write_all(report.as_bytes()).await
                })?;
            }

            // Fan on
            "M106" => {
                let speed = params
                    .remove(&'S')
                    .ok_or_else(|| err_msg("M106 requires S parameter"))?
                    .to_f32()?;

                if speed < 0.0 || speed > 255.0 {
                    return Err(err_msg("Invalid fan speed"));
                }

                lock!(state <= shared.state.lock().await?, {
                    state.part_cooling_fan_value = speed / 255.0;
                });
            }

            // Fan off
            "M107" => {
                lock!(state <= shared.state.lock().await?, {
                    state.part_cooling_fan_value = 0.0;
                });
            }

            // Set extruder temperature and wait.
            "M109" => {
                let temp = params
                    .remove(&'S')
                    .ok_or_else(|| err_msg("M109 requires S parameter"))?
                    .to_f32()?;

                lock!(state <= shared.state.lock().await?, {
                    state.extruder_target_temperature = temp;
                });

                loop {
                    let done = lock!(state <= shared.state.lock().await?, {
                        state.extruder_temperature == state.extruder_target_temperature
                    });

                    if done {
                        break;
                    }

                    executor::sleep(Duration::from_millis(10)).await?;
                }
            }

            // Configure debug flags
            "M111" => {
                params.clear();
            }

            // Print capabilitites
            "M115" => {
                lock_async!(writer <= shared.serial_writer.lock().await?, {
                    writer.write_all(b"Cap:AUTOREPORT_TEMP:1\n").await?;
                    writer.write_all(b"Cap:AUTOREPORT_FANS:1\n").await?;
                    writer.write_all(b"Cap:AUTOREPORT_POSITION:1\n").await
                })?;
            }

            // Set bed temperature
            "M140" => {
                let temp = params
                    .remove(&'S')
                    .ok_or_else(|| err_msg("M140 requires S parameter"))?
                    .to_f32()?;

                lock!(state <= shared.state.lock().await?, {
                    state.heatbed_target_temperature = temp;
                });
            }

            // M155 S1 C7
            "M155" => {
                let interval_secs = params
                    .remove(&'S')
                    .ok_or_else(|| err_msg("M155 requires S parameter"))?
                    .to_f32()?;

                // TODO: Interpret this value.
                let flags = params
                    .remove(&'C')
                    .ok_or_else(|| err_msg("M155 requires C parameter"))?
                    .to_f32()?;

                lock!(state <= shared.state.lock().await?, {
                    state.auto_report_interval = Duration::from_secs_f32(interval_secs);
                });
            }

            // Set bed temperature and wait.
            "M190" => {
                let temp = params
                    .remove(&'S')
                    .ok_or_else(|| err_msg("M190 requires S parameter"))?
                    .to_f32()?;

                lock!(state <= shared.state.lock().await?, {
                    state.heatbed_target_temperature = temp;
                });

                // TODO: Make a helper function for this.
                loop {
                    let done = lock!(state <= shared.state.lock().await?, {
                        state.heatbed_temperature == state.heatbed_target_temperature
                    });

                    if done {
                        break;
                    }

                    executor::sleep(Duration::from_millis(10)).await?;
                }
            }

            // Set max acceleration
            "M201" => {
                params.clear();
            }
            // Set max feed rate
            "M203" => {
                params.clear();
            }
            // Set default acceleration
            "M204" => {
                params.clear();
            }
            // Advanced settings like axis jerk limits.
            "M205" => {
                params.clear();
            }
            // Extrude factor overrides
            "M221" => {
                params.clear();
            }
            // Check nozzle diameter (Prusa specific)
            "M862.1" => {
                params.clear();
            }
            // Linear advance configuration
            "M900" => {
                params.clear();
            }
            // Motor current trimming
            "M907" => {
                params.clear();
            }
            _ => {
                return Err(format_err!("Unknown command: {}", cmd.as_str()));
            }
        }

        if !params.is_empty() {
            return Err(err_msg("Unsupported params in command"));
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
        let mut t = current_temp + (target_temp - current_temp) * dt;
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
