// Utilities for parsing the response lines written to the serial line by a CNC.

use base_error::*;
use cnc_monitor_proto::cnc::*;
use common::fixed::vec::FixedVec;

// TODO: Allow either "error" or "errors"?
regexp!(RESPONSE_STATUS_PREFIX => "^(?:(ok)|(error))(?:\\s|:|$)", "i");

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
    },
    Capability {
        name: String,
        present: bool,
    },
    AxisValue {
        id: String,
        values: FixedVec<f32, 2>,
    },
}

pub fn parse_response_line(
    mut line: &[u8],
    config: &MachineConfig,
    events: &mut Vec<ResponseEvent>,
) -> Result<()> {
    let mut whole_line_remaining = true;

    if let Some(m) = RESPONSE_STATUS_PREFIX.exec(line) {
        line = &line[m.last_index()..];
        whole_line_remaining = false;

        if m.group(1).is_some() {
            events.push(ResponseEvent::Ok);
        } else if m.group(2).is_some() {
            let message = bytes_to_string(line);
            events.push(ResponseEvent::Error { message });
            return Ok(());
        }
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
                // TODO: Parse a "/"

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

    use crate::{presets::get_prusa_i3_mk3sp_config, serial_receiver_buffer::SerialReceiverBuffer};

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
    fn error_parse() -> Result<()> {
        let config = get_prusa_i3_mk3sp_config()?;

        let line = b"error: Invalid line received";

        let mut events = vec![];
        parse_response_line(&line[..], &config, &mut events)?;

        // TODO: Assert [Error { message: " Invalid line received" }]

        println!("{:?}", events);

        Ok(())
    }

    #[testcase]
    async fn prusa_i3_log_parsing() -> Result<()> {
        let config = get_prusa_i3_mk3sp_config()?;

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

    #[testcase]
    async fn prusa_xl_log_parsing() -> Result<()> {
        // TODO:

        Ok(())
    }
}
