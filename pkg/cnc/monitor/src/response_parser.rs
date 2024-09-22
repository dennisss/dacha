// Utilities for parsing the response lines written to the serial line by a CNC.

use std::collections::HashMap;

use base_error::*;
use cnc_monitor_proto::cnc::*;
use common::{fixed::vec::FixedVec, hash::FastHasherBuilder};

// TODO: Allow either "error" or "errors"?
// NOTE: Alarm is only a grbl/smoothieware concept.
regexp!(RESPONSE_STATUS_PREFIX => "^(?:(ok)|(error)|(alarm))(?:\\s|:|$)", "i");

regexp!(CAPABILITY_LINE => "^Cap:([^:]+):([01])$");

regexp!(TAG_PATTERN => "^\\s*([0-9a-zA-Z_@\\-]+):");

// NOTE: This is very permissive of what format we will accept floats in
// All of these are allowed: "1", "-1" "0.00" "0..1" or ".1"
regexp!(FLOAT_PATTERN => "^\\s*(-)?0*([0-9]+)(?:\\.+([0-9]+)?)?(?:\\s|$)");

regexp!(SLASH_PATTERN => "^\\s*/");

regexp!(RPM_PATTERN => "^\\s*RPM");

#[derive(Clone, Debug)]
pub enum ResponseEvent {
    Ok,
    Error {
        message: String,
    },
    Echo {
        message: String,
        level: LogLevel,
    },
    Capability {
        name: String,
        present: bool,
    },
    AxisValue {
        id: String,
        values: FixedVec<f32, 2>,
        coordinate_system: CoordinateSystemIndex,
    },

    /// Generic event to indicate that this line contains primarily state
    /// information which may or may have been parsed out into the other events.
    IsStateUpdate,

    ///
    AxisOffset {
        id: String,
        offset: f32,
        coordinate_system: CoordinateSystemIndex,
    },
    FirmwareState {
        /// Name of the gRBL/Smoothieware state of the machine.
        name: String,
    },
    /// Report from the machine firmware indicating which tool is currently
    /// selected. Note that this may indicate a -1 if there is no tool selected.
    CurrentToolIndex {
        index: i32,
    },
    CurrentCoordinateSystem {
        index: u32,
    },

    SpindleMode(SpindleState_Mode),
    CurrentSpindleSpeed(f32),
    TargetSpindleSpeed(f32),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CoordinateSystemIndex {
    Machine,
    Current,
    Specific(u32),
}

#[derive(Clone, Copy, Debug)]
pub enum LogLevel {
    Error,
    Info,
    Debug,
}

pub fn parse_response_line(
    mut line: &[u8],
    config: &MachineConfig,
    events: &mut Vec<ResponseEvent>,
) -> Result<()> {
    match config.firmware() {
        MachineConfig_Firmware::MARLIN => parse_response_line_marlin(line, config, events),
        MachineConfig_Firmware::GRBL
        | MachineConfig_Firmware::SMOOTHIEWARE
        | MachineConfig_Firmware::CARVERA => parse_response_line_grbl(line, config, events),
        _ => Err(format_err!("Unsupported firmware")),
    }
}

fn parse_response_line_marlin(
    mut line: &[u8],
    config: &MachineConfig,
    events: &mut Vec<ResponseEvent>,
) -> Result<()> {
    // TODO: Add spindle status reporting for marlin.

    let mut whole_line_remaining = true;

    if let Some(remaining) = parse_response_status(line, events) {
        line = remaining;
        whole_line_remaining = false;
    }

    if whole_line_remaining {
        // TODO: Restrict this if M115 isn't supported.
        // https://reprap.org/wiki/G-code#M115:_Get_Firmware_Version_and_Capabilities
        if let Some(m) = CAPABILITY_LINE.exec(line) {
            let name = m.group_str(1).unwrap()?.to_string();
            let present = m.group_str(2).unwrap()? == "1";
            events.push(ResponseEvent::Capability { name, present });
            return Ok(());
        }

        if let Some(rest) = line.strip_prefix(b"echo:") {
            events.push(ResponseEvent::Echo {
                message: bytes_to_string(rest),
                level: LogLevel::Info,
            });
            return Ok(());
        }
    }

    let mut initial_num_events = events.len();

    // Parsing axis position data.
    while !line.is_empty() {
        // Ignore Marlin axis step counts
        if line.starts_with(b"Count ") {
            line = &[];
            break;
        }

        if let Some(m) = TAG_PATTERN.exec(line) {
            line = &line[m.last_index()..];

            let id = m.group_str(1).unwrap()?;

            let axis_config = match config.axes().iter().find(|a| a.id() == id) {
                Some(v) => v,
                None => break,
            };

            let mut values = FixedVec::new();

            let (v, rest) = parse_float(line)?;
            line = rest;
            values.push(v);

            // TODO: While heating, Marlin will emit additional lines of the form:
            // "T:206.43 E:0 B:49.4" which don't show the target temperatures.
            if axis_config.typ() == AxisType::HEATER {
                {
                    let m = SLASH_PATTERN
                        .exec(line)
                        .ok_or_else(|| format_err!("Missing / after heater value for: {}", id))?;
                    line = &line[m.last_index()..];
                }

                let (v, rest) = parse_float(line)?;
                line = rest;
                values.push(v);
            }

            if axis_config.typ() == AxisType::FAN_TACHOMETER_RPM {
                let m = RPM_PATTERN
                    .exec(line)
                    .ok_or_else(|| err_msg("Missing RPM unit after tachometer value"))?;
                line = &line[m.last_index()..];
            }

            // TODO: Raise an error if the same axis is updated multiple times on the same
            // line.
            events.push(ResponseEvent::AxisValue {
                id: id.to_string(),
                values,
                coordinate_system: CoordinateSystemIndex::Machine,
            });
            continue;
        }

        break;
    }

    if initial_num_events != events.len() && !line.is_empty() {
        return Err(format_err!(
            "Parsed some but not all data from a axis position line. Remaining: {}",
            bytes_to_string(line)
        ));
    }

    Ok(())
}

fn parse_response_status<'a>(
    mut line: &'a [u8],
    events: &mut Vec<ResponseEvent>,
) -> Option<&'a [u8]> {
    let m = match RESPONSE_STATUS_PREFIX.exec(line) {
        Some(v) => v,
        None => return None,
    };

    line = &line[m.last_index()..];

    if m.group(1).is_some() {
        events.push(ResponseEvent::Ok);
    } else if m.group(2).is_some() {
        let message = bytes_to_string(line);
        events.push(ResponseEvent::Error { message });
        line = &[];
    } else if m.group(3).is_some() {
        // NOTE: In gRBL, the message should be a number, but in other
        // firmwares like Smoothieware, this may be a human readable
        // message.
        let message = bytes_to_string(line);
        line = &[];
        events.push(ResponseEvent::Echo {
            message,
            level: LogLevel::Error,
        });
    }

    Some(line)
}

fn parse_response_line_grbl(
    mut line: &[u8],
    config: &MachineConfig,
    events: &mut Vec<ResponseEvent>,
) -> Result<()> {
    // Used to indicate start up lines (and Smoothieware macros were one gcode
    // internally runs many gcodes).
    if line.starts_with(b">") {
        return Ok(());
    }

    if let Some(inner) = strip_wrapper(line, b'<', b'>') {
        let s = std::str::from_utf8(inner)?;

        let (state, params) = parse_grbl_report(s, true)?;
        events.push(ResponseEvent::FirmwareState {
            name: state.to_string(),
        });

        if let Some(params) = params.get("MPos") {
            for (idx, axis) in ["X", "Y", "Z" /* , "A", "B" */].iter().enumerate() {
                if params.len() <= idx {
                    break;
                }

                let mut values = FixedVec::new();
                values.push(params[idx].parse()?);

                events.push(ResponseEvent::AxisValue {
                    id: axis.to_string(),
                    values,
                    coordinate_system: CoordinateSystemIndex::Machine,
                });
            }
        }

        // TODO: Dedup this.
        if let Some(params) = params.get("WPos") {
            for (idx, axis) in ["X", "Y", "Z" /* , "A", "B" */].iter().enumerate() {
                if params.len() <= idx {
                    break;
                }

                let mut values = FixedVec::new();
                values.push(params[idx].parse()?);

                events.push(ResponseEvent::AxisValue {
                    id: axis.to_string(),
                    values,
                    coordinate_system: CoordinateSystemIndex::Current,
                });
            }
        }

        // Carvera specific
        // T: "tool index", "tool length/offset"
        if let Some(params) = params.get(&"T") {
            if params.len() < 2 {
                return Err(err_msg("Too few parameters in T report"));
            }

            let index = params[0].parse::<i32>()?;
            events.push(ResponseEvent::CurrentToolIndex { index });
        } else {
            // The stock carvera firmware won't print out the tool index if no tool is
            // selected.
            events.push(ResponseEvent::CurrentToolIndex { index: -1 });
        }

        // Smoothieware
        // F: "current feedrate", "requested feedrate",

        // Carvera
        // S: "current_rpm", "target_rpm" (second parameter only )
        if let Some(params) = params.get(&"S") {
            if params.len() < 2 {
                return Err(err_msg("Too few spindle status params"));
            }

            events.push(ResponseEvent::CurrentSpindleSpeed(params[0].parse()?));
            events.push(ResponseEvent::TargetSpindleSpeed(params[1].parse()?));
        }

        extract_axis_values(&params, "STATUS:", config, events)?;

        // gRBL
        // FS: "current feed rate", "current spindle rpm"

        // gRBL
        // F: "current feed rate"

        // TODO: Need to expose  STATUS parameters for getting the wireless probe
        // voltage.

        return Ok(());
    }

    /*
    The first value for switches is the on/off state.
    Second one is a PWM value if supported.

    Smoothieware switch on/off commands have an "S" parameter which can be the PWM from 0 to 1 or -1 to use the default value.
     */

    if let Some(inner) = strip_wrapper(line, b'{', b'}') {
        let s = std::str::from_utf8(inner)?;

        let (_, params) = parse_grbl_report(s, false)?;

        if let Some(params) = params.get(&"S") {
            if params.len() < 2 {
                return Err(err_msg("Too few spindle diagnostic parameters"));
            }

            let mode = match params[0].parse::<usize>()? {
                0 => SpindleState_Mode::OFF,
                1 | _ => SpindleState_Mode::ON_CLOCKWISE,
            };
            events.push(ResponseEvent::SpindleMode(mode));

            let target_rpm = params[1].parse::<f32>()?;
            events.push(ResponseEvent::TargetSpindleSpeed(target_rpm));
        }

        extract_axis_values(&params, "DIAG:", config, events)?;

        // TODO: Alert if there are any unused parameters.

        return Ok(());
    }

    if let Some(inner) = strip_wrapper(line, b'[', b']') {
        let s = std::str::from_utf8(inner)?;

        if let Some((prefix, values)) = s.split_once(':') {
            match prefix {
                "MSG" => {
                    events.push(ResponseEvent::Echo {
                        message: values.trim().to_string(),
                        level: LogLevel::Info,
                    });
                }
                "echo" => {
                    events.push(ResponseEvent::Echo {
                        message: values.trim().to_string(),
                        level: LogLevel::Debug,
                    });
                }
                "G92" => {
                    // Current offset of all the coordinate systems (including
                    // the world coordinates). Hopefully should always be 0.
                    events.push(ResponseEvent::IsStateUpdate);
                }
                "PRB" => {
                    // Probe information?
                    events.push(ResponseEvent::IsStateUpdate);
                }
                "TL0" | "G28" | "G30" => {
                    events.push(ResponseEvent::IsStateUpdate);
                }
                /*
                TODO: Sometimes we get:
                    "[G55:0.0000,*.0000,0.0000]" which is unparseable.

                 */
                _ => {
                    if let Some(idx) = gcode::STANDARD_COORDINATE_SYSTEMS
                        .iter()
                        .position(|v| *v == prefix)
                    {
                        let mut values = values.split(',');

                        for axis in ["X", "Y", "Z"] {
                            let v = values
                                .next()
                                .ok_or_else(|| err_msg("Too few coordinate system offsets"))?
                                .parse::<f32>()?;

                            events.push(ResponseEvent::AxisOffset {
                                id: axis.to_string(),
                                offset: v,
                                coordinate_system: CoordinateSystemIndex::Specific(idx as u32 + 1),
                            });
                        }

                        if values.next().is_some() {
                            return Err(err_msg(
                                "Expected exactly three values in coordinate system offset",
                            ));
                        }
                    } else {
                        // eprintln!("Unknown message prefix: {}", prefix);
                    }
                }
            }

            return Ok(());
        }

        // Else most likely this is the output of '$G' (list of GCode words showing the
        // current state).
        parse_gcode_state_report(s, events)?;

        return Ok(());
    }

    let mut whole_line_remaining = true;

    if let Some(remaining) = parse_response_status(line, events) {
        line = remaining;
        whole_line_remaining = false;
    }

    // TODO: Add parsing of bytes after 'ok'

    Ok(())
}

fn extract_axis_values(
    params: &HashMap<&str, Vec<&str>, FastHasherBuilder>,
    params_prefix: &str,
    config: &MachineConfig,
    events: &mut Vec<ResponseEvent>,
) -> Result<()> {
    for axis in config.axes() {
        if axis.value().is_empty() || !axis.value()[0].starts_with(params_prefix) {
            continue;
        }

        let mut values = FixedVec::new();

        for (i, value_path) in axis.value().iter().enumerate() {
            let value_path = value_path.strip_prefix(params_prefix).unwrap();
            let (field, rest) = value_path.split_once(':').unwrap();
            let field_index = rest.parse::<usize>()?;

            let params = match params.get(&field) {
                Some(v) => v,
                None => {
                    if axis.default_value().len() > i {
                        values.push(axis.default_value()[i]);
                    }
                    continue;
                }
            };

            if params.len() <= field_index {
                if axis.default_value().len() > i {
                    values.push(axis.default_value()[i]);
                    continue;
                }

                return Err(format_err!(
                    "Not enough fields for diagnostic parameter {}",
                    field
                ));
            }

            values.push(params[field_index].parse()?);
        }

        events.push(ResponseEvent::AxisValue {
            id: axis.id().to_string(),
            values,
            coordinate_system: CoordinateSystemIndex::Machine,
        });
    }

    Ok(())
}

/// Parses the data inside of the <> brackets.
fn parse_grbl_report(
    s: &str,
    first_is_state: bool,
) -> Result<(&str, HashMap<&str, Vec<&str>, FastHasherBuilder>)> {
    let mut params = HashMap::<&str, Vec<&str>, FastHasherBuilder>::default();

    let mut parts = s.split('|');

    let state_str = {
        if first_is_state {
            parts
                .next()
                .ok_or_else(|| err_msg("No state string in status report"))?
                .trim()
        } else {
            ""
        }
    };

    for tuple in parts {
        let (key, values) = tuple
            .split_once(':')
            .ok_or_else(|| format_err!("Invalid status report tuple: {}", tuple))?;

        params.insert(key.trim(), values.split(',').map(|s| s.trim()).collect());
    }

    Ok((state_str, params))
}

fn parse_gcode_state_report(s: &str, events: &mut Vec<ResponseEvent>) -> Result<()> {
    let mut command_words = vec![];
    let mut params = gcode::LineParameters::default();

    let mut parser = gcode::Parser::new();

    let mut remaining = s.as_bytes();
    while !remaining.is_empty() {
        let (e, n) = parser.next(remaining, true);
        remaining = &remaining[n..];

        let e = match e {
            Some(v) => v,
            None => break,
        };

        match e {
            gcode::Event::Word(w) => {
                if w.key == 'T' {
                    // NOTE: This is a property of the gcode parser but not
                    // reflective of the actual selected tool.
                    // TODO: check this in grbl
                    /*
                    events.push(ResponseEvent::CurrentToolIndex {
                        index: w.value.to_f32()? as i32,
                    });
                    */
                } else if w.key == 'G' || w.key == 'M' {
                    command_words.push(gcode::CommandWord::from_word(&w)?);
                } else {
                    params.add_param(w.key, w.value)?;
                }
            }
            gcode::Event::EndLine => {}
            _ => {
                return Err(err_msg("Bad format of gcode parser state line"));
            }
        }
    }

    for command in command_words {
        if command == gcode::SpindleOnClockwise::COMMAND {
            // TODO: These are not applicable to carvera machines.
            /*
            let speed = params
                .take_param('S')?
                .ok_or_else(|| err_msg("Missing spindle speed"))?
                .to_f32()?;
            let mut state = SpindleState::default();
            state.set_on(true);
            state.set_clockwise(true);
            state.set_target_speed_rpm(speed);
            events.push(ResponseEvent::SpindleState(state));
            */
        } else if command == gcode::SpindleOnCounterClockwise::COMMAND {
            /*
            let speed = params
                .take_param('S')?
                .ok_or_else(|| err_msg("Missing spindle speed"))?
                .to_f32()?;
            let mut state = SpindleState::default();
            state.set_on(true);
            state.set_clockwise(false);
            state.set_target_speed_rpm(speed);
            events.push(ResponseEvent::SpindleState(state));
            */
        } else if command == gcode::SpindleOff::COMMAND {
            /*
            let speed = params
                .take_param('S')?
                .ok_or_else(|| err_msg("Missing spindle speed"))?
                .to_f32()?;
            let mut state = SpindleState::default();
            state.set_on(false);
            state.set_clockwise(false);
            state.set_target_speed_rpm(speed);
            events.push(ResponseEvent::SpindleState(state));
            */
        } else if let Some(idx) = gcode::STANDARD_COORDINATE_SYSTEM_CODES
            .iter()
            .position(|v| v == &command)
        {
            events.push(ResponseEvent::CurrentCoordinateSystem {
                index: idx as u32 + 1,
            });
        }
    }

    Ok(())
}

fn strip_wrapper(input: &[u8], open_bracket: u8, close_bracket: u8) -> Option<&[u8]> {
    if input.len() < 2 || input[0] != open_bracket || input[input.len() - 1] != close_bracket {
        return None;
    }

    Some(&input[1..(input.len() - 1)])
}

fn parse_float(input: &[u8]) -> Result<(f32, &[u8])> {
    let m = FLOAT_PATTERN
        .exec(input)
        .ok_or_else(|| err_msg("Failed to find the float pattern"))?;

    let normalized = format!(
        "{}{}.{}",
        m.group_str(1).unwrap_or(Ok(""))?,
        m.group_str(2).unwrap_or(Ok("0"))?,
        m.group_str(3).unwrap_or(Ok("0"))?
    );

    let v = normalized.parse::<f32>()?;

    Ok((v, &input[m.last_index()..]))
}

fn bytes_to_string(input: &[u8]) -> String {
    let mut out = String::new();
    out.reserve(input.len());

    for b in input {
        if b.is_ascii_graphic() || *b == b' ' {
            out.push(*b as char);
        } else {
            out.push_str(&format!("\\x{:02x}", b));
        }
    }

    out
}

#[cfg(test)]
mod tests {

    use std::time::Instant;

    use crate::presets::*;
    use crate::serial_receiver_buffer::SerialReceiverBuffer;

    use super::*;

    /*
    Types of lines to handle:
    - "echo:busy: paused for user"
    - "ERROR:"
    - "error"
    - "error: message"
    - "ok"
    - "Cap:AUTOREPORT_POSITION:1"
    - "ok T:23.8 /0.0 B:24.9 /0.0 T0:23.8 /0.0 @:0 B@:0 P:0.0 A:30.7"

    - "ok T:20.2 /0.0 B:19.1 /0.0 T0:20.2 /0.0 @:0 B@:0 P:19.8 A:26.4"

    - "Cap:SOFTWARE_POWER:0"


    Prusa specific gcodes:
    -

    - "T:24.0 /0.0 B:24.7 /0.0 T0:24.0 /0.0 @:0 B@:0 P:0.0 A:31.2"
        - From M105

    - "X:0.00 Y:0.00 Z:0.15 E:0.00 Count X: 0.00 Y:0.00 Z:0.15 E:0.00"
        - This is absolute position and step counts.

    - "E0:0 RPM PRN1:0 RPM E0@:0 PRN1@:0"
        - M123 Tachometer value
        - E0: - Hotend fan speed in RPM
        - PRN1: - Part cooling fans speed in RPM
        - E0@: - Hotend fan PWM value
        - PRN1@: -Part cooling fan PWM value

    - "X:0.00 Y:127.00 Z:145.00 E:0.00 Count X: 0 Y:10160 Z:116000"
    */

    #[testcase]
    async fn error_parse() -> Result<()> {
        let config = get_prusa_i3_mk3sp_config().await?;

        let line = b"error: Invalid line received";

        let mut events = vec![];
        parse_response_line(&line[..], &config, &mut events)?;

        // TODO: Assert [Error { message: " Invalid line received" }]

        println!("{:?}", events);

        Ok(())
    }

    #[testcase]
    async fn prusa_i3_log_parsing() -> Result<()> {
        let config = get_prusa_i3_mk3sp_config().await?;

        // Grabbed from a Prusa I3 MK3s on startup and issuing a a few commands.
        let log: &'static [&'static [u8]] = &[
            b"start\n",
            b"echo: 3.13.3-7094\nSpoo",
            b"lJoin is Off\necho: Last Updated: Feb 27 2024 18:",
            b"19:31 | Author: (none, default config)\necho: Fre",
            b"e Memory: 2517  PlannerBufferBytes: 1760\n",
            b"echo:Stored settings retrieved\n",
            b"adc_ini",
            b"t\nHotend fan type: ",
            b"NOCTUA\nCrashDetect DISA",
            b"BLED\n",
            b"Sendin",
            b"g 0xFF\n",
            b"echo:SD card ok\n",
            // Send M123
            b"E0:0 RPM PRN1:0 RPM E0@:0 PRN1@:0\nok\n",
            // Send M114
            b"Command not found!\n",
            b"X:0.00 Y:0.00 Z:0.15 E:0.",
            b"00 Count X: 0.00 Y:0.00 Z:0.15 E:0.00\nok\n",
            // Send M105
            b"ok T:21.8 /0.0 B:22.1 /0.0 T0:21.8 /0.0 @:0",
            b" B@:0 P:0.0 A:25.3\n",
            // Send M115
            b"FIRMWARE_NAME:Prusa-F",
            b"irmware 3.13.3 based on Marlin FIRMWARE_URL:https",
            b"://github.com/prusa3d/Prusa-Firmware PROTOCOL_VE",
            b"RSION:1.0 MACHINE_TYPE:Prusa i3 MK3S EXTRUDER_CO",
            b"UNT:1 UUID:00000000-0000-0000-0000-000000000000\n",
            b"Cap:AUTOREPORT_TEMP:1\nCap:AUTOREPORT_FANS:1\nCap:",
            b"AUTOREPORT_POSITION:1\nCap:EXTENDED_M20:1\nCap:PRUS",
            b"A_MMU2:1\nok\n",
        ];

        let buffer = SerialReceiverBuffer::default();

        for buf in log {
            buffer.append(*buf, Instant::now()).await?;
        }

        let num_lines = buffer.last_line_offset().await?;
        assert_eq!(num_lines, 24);

        for i in 0..num_lines {
            let line = buffer.get_line(i).await?;

            println!("==> {:?}", line);

            let mut events = vec![];
            parse_response_line(&line.data, &config, &mut events)?;

            println!("{:?}", events);
        }

        Ok(())
    }

    /*
    Log from Prusa XL when starting a print file from SDCard:

        b"echo:endstops hit:  Z:13.37\n"
    b"echo:Probe classified as clean and OK\n"
    b"echo:Starting probe at 1\n"
    b"echo:busy: processing\n"
    b"echo:endstops hit:  Z:16.46\n"
    b"echo:Probe classified as clean and OK\n"
    b"echo:Starting probe at 1\n"
    b"echo:busy: processing\n"
    b"echo:endstops hit:  Z:19.54\n"
    b"echo:Probe classified as clean and OK\n"
    b"X:195.43 Y:102.86 Z:2.00 E:-2.00 Count A:23863 B:7405 Z:1600\n"
    b"echo:busy: processing\n"
    b"echo:Starting probe at 1\n"
    b"echo:busy: processing\n"
    b"echo:busy: processing\n"
    b"echo:endstops hit:  Z:7.20\n"
    b"echo:Probe classified as clean and OK\n"
    b"echo:Starting probe at 1\n"
    b"echo:busy: processing\n"
    b"echo:endstops hit:  Z:4.11\n"
    b"echo:Probe classified as clean and OK\n"
    b"X:41.14 Y:10.29 Z:2.00 E:-2.00 Count A:4114 B:2468 Z:1600\n"
    b"Extrapolating mesh...done\nUnified Bed Leveling System v1.01 active\n"
    b"echo:busy: processing\n"
    b"echo:busy: processing\n"
    b"echo:busy: processing\n"
    b"echo:busy: processing\n"
    b" T:175.00/240.00 B:80.10/80.00 C:-30.00/0.00 X0:42.00/36.00 A:59.68/0.00"
    b" T0:175.00/240.00 T1:27.00/0.00 T2:27.00/0.00 T3:27.00/0.00 T4:26.00/0.00"
    b" T5:6.00/0.00 @:42 B@:0 HBR@:255 @0:42 @1:0 @2:0 @3:0 @4:0 @5:0 B_0_0:59.90/60.00 B_1_0:60.00/60.00 B_2_0:60.00/60.00 B_3_0:60.0"
    b"0/60.00 B_0_1:80.00/80.00 B_1_1:80.10/80.00 B_2_1:80.10/80.00 B_3_1:80.00/80.00 B_0_2:60.00/60.00 B_1_2:80.20/80.00 B_2_2:80.10/"
    b"80.00 B_3_2:60.00/60.00 B_0_3:40.10/40.00 B_1_3:60.10/60.00 B_2_"
    b"3:60.00/60.00 B_3_3:40.10/40.00 W:?\n"
    b" T:175.00/240.00 B:80.12/80.00 C:-30.00/0.00 X0:42.00/36.00 A:60.24/0.00"
    b" T0:175.00/240.00 T1:26.00/0.00 T2:27.00/0.00 T3:27.00/0.00 T4:26.00/0.00"
    b" T5:6.00/0.00 @:117 B@:0 HBR@:255 @0:117 @1:0 @2:0 @3:0 @4:0 @5:0 B_0_0:59.90/60.00 B_1_0:60.00/60.00 B_2_0:60.00/60.00 B_3_0:60"
    b".00/60.00 B_0_1:80.00/80.00 B_1_1:80.10/80.00 B_2_1:80.10/80.00 B_3_1:80.00/80.00 B_0_2:60.00/60.00 B_1_2:80.20/80.00 B_2_2:80.1"
    b"0/80.00 B_3_2:60.00/60.00 B_0_3:40.10/40.00 B_1_3:60.10/60.00 B_2_3:60.00/60.00 B_3_3:40.10/40.00 W:?\n"
    b"echo:busy: processing\n"
    b" T:174.00/240.00 B:80.12/80.00 C:-30.00/0.00 X0:42.00/36.00 A:60.18/0.00"
    b" T0:174.00/240.00 T1:26.00/0.00 T2:27.00/0.00 T3:27.00/0.00 T4:26.00/0.00"
    b" T5:6.00/0.00 @:118 B@:0 HBR@:255 @0:118 @1:0 @2:0 @3:0 @4:0 @5:0 B_0_0:59.90/60.00 B_1_0:60.00/60.00 B_2_0:60.00/60.00 B_3_0:60"
    b".00/60.00 B_0_1:80.00/80.00 B_1_1:80.10/80.00 B_2_1:80.10/80.00 B_3_1:80.00/80.00 B_0_2:60.00/60.00 B_1_2:80.20/80.00 B_2_2:80.10/80.00 B_3_2:60.00/60.00 B_0_3:40.10/40.00 B_1_3:60.10/60.00 B_"
    b"2_3:60.10/60.00"
    b" B_3_3:40.20/40.00 W:?\n"
    b" T:175.00/240.00 B:80.15/80.00 C:-30.00/0.00 X0:42.00/36.00 A:59.93/0.00"
    b" T0:175.00/240.00 T1:27.00/0.00 T2:27.00/0.00 T3:27.00/0.00 T4:26.00/0.00"
    b" T5:6.00/0.00 @:115 B@:0 HBR@:255 @0:115 @1:0 @2:0 @3:0 @4:0 @5:0 B_0_0:59.90/60.00 B_1_0:60.00/60.00 B_2_0:60.00/60.00 B_3_0:60.00/60.00 B_0_1:80.00/80.00 B_1_1:80.20/80.00 B_2_1:80.10/80.00 "
    b"B_3_1:80.00/80.00"
    b" B_0_2:60.00/60.00 B_1_2:80.20/80.00 B_2_2:80.10/80.00 B_3_2:60.00/60.00 B_0_3:40.10/40.00 B_1_3:60.10/60.00 B_2_3:60.00/60.00 B_3_3:40.10/40.00 W:?\n"
    b"echo:busy: processing\n"
    b"echo:busy: processing\n"
    b" T:241.00/240.00 B:80.07/80.00 C:-30.00/0.00 X0:41.00/36.00 A:59.17/0.00"
    b" T0:241.00/240.00 T1:26.00/0.00 T2:27.00/0.00 T3:27.00/0.00 T4:26.00/0.00"
    b" T5:6.00/0.00 @:61 B@:0 HBR@:255 @0:61 @1:0 @2:0 @3:0 @4:0 @5:0 B_0_0:60.10/60.00 B_1_0:60.00/60.00 B_2_0:60.10/60.00 B_3_0:60.00/60.00 B_0_1:80.10/80.00 B_1_1:80.10/80.00 B_2_1:80.10/80.00 B_"
    b"3_1:80.00/80.00"
    b" B_0_2:60.00/60.00 B_1_2:80.10/80.00 B_2_2:80.00/80.00 B_3_2:60.00/60.00 B_0_3:40.10/40.00 B_1_3:60.00/60.00 B_2_3:60.00/60.00 B_3_3:40.30/40.00"
    b" W:0\n"
    b"echo:busy: processing\n"
    b"X:14.90 Y:360.00 Z:10.36 E:0.00 Count A:29992 B:-27608 Z:8446\n"
    b"echo:busy: processing\n"
    b"X:76.00 Y:-7.00 Z:0.20 E:0.00 Count A:9067 B:-4888 Z:8220\n"
    b"echo:busy: processing\n"
    b"echo:busy: processing\n"


    // M115
    b"FIRMWARE_NAME:Prusa-Firmware-Buddy 6.0.3+14902 (Github) SOURCE_CODE_URL:https://github.com/prusa3d/Prusa-Firmware-Buddy PROTOCOL"
    b"_VERSION:1.0 MACHINE_TYPE:Prusa-XL EXTRUDER_COUNT:5 UUID:cede2a2f-41a2-4748-9b12-c55c62f367ff\nCap:SERIAL_XON_XOFF:0\r\nCap:BINARY_"
    b"FILE_TRANSFER:0\r\nCap:EEPROM:0\r\nCap:VOLUMETRIC:1\r\nCap:AUTOREPORT_TEMP:1\r\nCap:PROGRESS:0\r\nCap:PRINT_JOB:1\r\nCap:AUTOLEVEL:1\r\nCap:Z_PROBE:1\r\nCap:LEVELING_DATA:1\r\nCap:BUILD_PERCENT:0\r\nCap:SOFTWARE_"
    b"POWER:0\r\n"
    b"Cap:TOGGLE_LIGHTS:0\r\nCap:CASE_LIGHT_BRIGHTNESS:0\r\nCap:EMERGENCY_PARSER:0\r\nCap:PROMPT_SUPPORT:0\r\nCap:AUTOREPORT_SD_STATUS:0\r\nCap:"
    b"THERMAL_PROTECTION:1\r\nCap:MOTION_MODES:0\r\nCap:CHAMBER_TEMPERATURE:0\r\nok\n"

    // M105
    b"ok T:240.00/240.00 B:80.07/80.00 C:-30.00/0.00 X0:50.00/36.00 A:59.90/0.00"
    b" T0:240.00/240.00 T1:27.00/0.00 T2:27.00/0.00 T3:28.00/0.00 T4:27.00/0.00"
    b" T5:6.00/0.00 @:62 B@:0 HBR@:255 @0:62 @1:0 @2:0 @3:0 @4:0 @5:0 B_0_0:60.00/60.00 B_1_0:60.00/60.00 B_2_0:60.00/60.00 B_3_0:60.0"
    b"0/60.00 B_0_1:80.00/80.00 B_1_1:80.00/80.00 B_2_1:80.00/80.00 B_3_1:80.00/80.00 B_0_2:60.00/60.00 B_1_2:80.30/80.00 B_2_2:80.00/80.00 B_3_2:60.00/60.00 B_0_3:41.30/40.00 B_1_3:60.00/60.00 B_2_"
    b"3:60.00/60.00"
    b" B_3_3:41.20/40.00\n"


    // M114
    b"echo:busy: processing\n"
    b"X:135.62 Y:136.28 Z:0.20 E:364.44 Count A:21760 B:-49 Z:200\n"
    b"ok\n"

    // M123
    b"E0:7835 RPM PRN1:0 RPM E0@:255 PRN1@:0\n"
    b"\nok\n"

    // M863
    b"Tool mapping: \r\n"
    b"  Tool 0 -> 0\n  Tool 1 -> <none>\n  Tool 2 -> <none>\n  Tool 3 -> <none>\n  Tool 4 -> <none>\n  Tool 5 -> 5\nEnabled: 1\nok\n"


    // M333
    b"echo:touch_evt1\n"
    b"echo:is_printing1\necho:active_extruder1\necho:temp_hbr0\necho:temp_brd0\necho:temp_chamber0\necho:temp_mcu0\necho:temp_sandwich0\necho:temp_splitter0\n"
    b"echo:temp_bed0\necho:ttemp_bed0\necho:temp_noz0\necho:ttemp_noz0\necho:fan_speed0\necho:fan_hbr_speed0\necho:ipos_x0\necho:ipos_y0\necho:ipos_z0\n"
    b"echo:pos_x0\necho:pos_y0\necho:pos_z0\necho:adj_z1\necho:fw_version1\necho:buddy_revision1\necho:buddy_bom1\necho:filament1\necho:stack1"
    b"\necho:runtime1\necho:heap1\necho:print_filename1\necho:dwarf_board_temp1\necho:dwarf_mcu_temp0\necho:dwarfs_mcu_temp0\necho:dwarfs_board_temp0\n"
    b"echo:power_panic1\necho:side_fsensor0\necho:fsensor0\necho:side_fsensor_raw0\necho:fsensor_raw0\necho:tmc_write1\necho:tmc_read1\necho:"
    b"eeprom_write1\n"
    b"echo:points_dropped1\necho:tmc_sg_e0\necho:tmc_sg_z0\necho:tmc_sg_y0\necho:tmc_sg_x0\necho:gui_loop_dur0\necho:modbus_reqfail1\necho:be"
    b"dlet_curr0\n"
    b"echo:bed_curr1\necho:bedlet_state0\necho:bedlet_temp0\necho:bedlet_pwm0\necho:bedlet_reg0\necho:bed_state0\necho:bed_mcu_temp0\necho:be"
    b"dlet_target0\n"
    b"echo:dwarf_fast_refresh_delay0\necho:dwarf_parked_raw0\necho:dwarf_picked_raw0\necho:dwarf_heat_curr0\necho:dwarf_heat_pwm0\necho:loa"
    b"dcell0\necho:loadcell_age0\necho:loadcell_value0\necho:loadcell_hp0\necho:loadcell_xy0\necho:app_start1\necho:maintask_loop0\necho:cpu_"
    b"usage1\n"
    b"echo:usbh_err_cnt1\necho:media_prefetched1\necho:print_fan_act0\necho:hbr_fan_act0\necho:hbr_fan_enc0\necho:touch_pos1\necho:splitter_5V_current1\n"
    b"echo:24VVoltage1\necho:5VVoltage1\necho:Sandwitch5VCurrent1\necho:xlbuddy5VCurrent1\necho:g425_rxy1\necho:g425_rz1\necho:g425_z1\necho:g425_xy1\necho:g425_xy_dev1\necho:gcode0\necho:loadcell_scale1\necho:loadcell_threshold1\n"
    b"echo:loadcell_threshold_cont1\necho:loadcell_hysteresis1\necho:g425_cen1\necho:g425_off1\necho:esp_out1\necho:eth_out1\necho:esp_in1\necho:eth_in1\n"
    b"echo:fan1\necho:home_diff1\necho:probe_analysis1\necho:probe_start1\necho:probe_z_diff0\necho:probe_z0\necho:tk_accel0\necho:freq_gain1\n"
    b"echo:excite_freq0\necho:crash_repeated1\necho:crash1\necho:crash_stat1\nok\n"


    To actually get the active extruder, you need to look at the active metrics:

    - https://github.com/prusa3d/Prusa-Firmware-Buddy/blob/f5a498ab8d2a42341d0dbeb969b7ae047783e860/src/common/app_metrics.cpp#L146C59-L146C79

        */

    #[testcase]
    async fn prusa_xl_log_parsing() -> Result<()> {
        // TODO:

        Ok(())
    }

    #[testcase]
    async fn makera_carvera_log_parsing() -> Result<()> {
        let config = get_makera_carvera_config().await?;

        let log: &'static [&'static [u8]] = &[
            b"version = 0.9.7\n",
            b"Watchdog enabled for 10.000 seconds\n",
            b">ok\nG28 means goto clearance position on CARVERA\n",
            b"STA connection timeout, disconnected!\n",
            // "M6T6\n"
            b"Start atc, old tool: T0, new tool: T6\r\n",
            b"ok\r\n>M497.1\r\n>ok\r\n>G53 G0 Z-3.000\r\n>ok\r\n>G53 G0 X-3.755 Y-24.290\r\n>ok\r\n>M492.2\r\n",
            b">ok\r\n>G53 G0 X-3.755 Y-24.290\r\n>ok\r\n>G53 G1 Z-97.230 F1000.000\r\n>ok\r\n>G53 G1 Z-112.230 F200.000\r\n>ok\r\n>M490.2\r\nHoming atc...\n",
            b"ATC homed!\r\n",
            b"ATC loosed!\r\n>ok\r\n>G53 G0 Z-50.000\r\n>ok\r\n>M493.2 T-",
            b"1\r\n",
            b">ok\r\n>M492.1\r\n",
            b">ok\r\n>M497.2\r\n",
            b">ok\r\n>G53 G0 Z-50.000\r\n>ok\r\n>G53 G0 X-3.755 Y-234.290\r\n",
            b">ok\r\n>",
            b"M492.1\r\n",
            b">ok\r\n>M490.2\r\nAlready loosed!\n>ok\r\n>G53 G0 X-3.755 Y-234.290\r\n>ok\r\n>G53 G1 Z-97.230 F1000.000\r\n>ok\r\n>G53 G1 Z-112.230 F200.000\r\n>ok\r\n>M490.1\r\n",
            b"ATC clamped!\r\n>ok\r\n>G53 G0 Z-20.000\r\n",
            b">ok\r\n>M492.2\r\n",
            b">ok\r\n>M493.2 T6\r\n",
            b">ok\r\n>M497.3\r\n>ok\r\n>",
            b"G53 G0 Z-20.000\r\n>ok\r\n>G53 G0 X-3.755 Y-54.290\r\n",
            b">ok\r\n>G38.6 Z-152.230 F500.000\r\n",
            b">[PRB:-3.755,-54.290,-87.715:1]\n",
            b">ok\r\n>G91 G0 Z2.000\r\n>ok\r\n>G38.6 Z-3.000 F100.000\r\n",
            b">[PRB:-3.755,-54.290,-87.690:1]\n>ok\r\n>M493.1\r\n",
            b">ok\r\n>G53 G0 Z-20.000\r\n>ok\r\n",
            b"Done ATC\r\n",
            // "?\n"
            // NOTE: This is using an extra non-standard 5th parameter for 'S'
            b"<Idle|MPos:-1.0000,-1.0000,-1.0000,0.0000,0.0000|WPos:359.1580,233.5680,127.0350|F:0.0,3000.0,100.0|S:1015.5,1000.0,100.0,1,27.6,0|T:6,-13.595|W:4.10|L:0, 0, 0, 0.0,100.0>\nok\r\n",
            // "M105\n" (query spindle temperature)
            b"ok M:26.1 /0.0 @0 \r\n",
            // "$G\n"
            b"[G0 G54 G17 G21 G90 G94 M0 M5 M9 T0 F3000.0000 S1.0000]\nok\n",
            // "$#\n"
            b"[G54:-307.7550,-196.7900,-61.3050]\n[G55:0.0000,0.0000,0.0000]\n[G56:0.0000,0.0000,0.0000]\n[G57:0.0000,0.0000,0.0000]\n[G58:0.0000,0.0000,0.0000]\n[G59:0.0000,0.0000,0.0000]\n[G59.1:0.0000,0.0000,0.0000]\n[G59.2:0.0000,0.0000,0.0000]\n[G59.3:0.0000,0.0000,0.0000]\n[G28:0.0000,0.0000,0.0000]\n[G30:0.0000,0.0000,0.0000]\n[G92:0.0000,0.0000,0.0000]\n[TL0:-13.4900]\n[PRB:0.0000,0.0000,0.0000:0]\nok\n",
            // "*\n"
            b"{S:1,1000|L:0,0|V:1,80|F:0,0|G:1|T:0|R:0|C:0|E:0,0,0,0,0,0|P:0,0|A:0,0|I:0}\nok\r\n"

            // TODO: M115
        ];

        let buffer = SerialReceiverBuffer::default();

        for buf in log {
            buffer.append(*buf, Instant::now()).await?;
        }

        let num_lines = buffer.last_line_offset().await?;
        // assert_eq!(num_lines, 24);

        for i in 0..num_lines {
            let line = buffer.get_line(i).await?;

            println!("==> {:?}", line.data);

            let mut events = vec![];
            parse_response_line(&line.data, &config, &mut events)?;

            println!("{:?}", events);
        }

        Ok(())
    }
}
