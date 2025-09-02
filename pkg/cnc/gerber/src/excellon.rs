use std::collections::HashMap;

use base_error::*;
use graphics::transforms::transform2f;
use math::matrix::{vec2f, Matrix3f};

regexp!(TOOL_INFO_LINE => "^T([0-9]+)C([0-9.]+)$");
regexp!(SELECT_TOOL_LINE => "^T([0-9]+)$");
regexp!(XY_LINE => "^X([0-9\\.\\-]+)Y([0-9\\.\\-]+)$");

#[derive(Debug)]
pub struct DrillFile {
    pub holes: Vec<DrillHole>,
}

#[derive(Debug, Clone)]
pub struct DrillHole {
    pub x: f32,
    pub y: f32,
    pub diameter: f32,
}

impl DrillHole {
    pub fn transform(&mut self, transform: &Matrix3f) {
        let v = transform2f(transform, &vec2f(self.x, self.y));
        self.x = v.x();
        self.y = v.y();
    }
}

impl DrillFile {
    pub fn parse_excellon(data: &[u8]) -> Result<Self> {
        let data = std::str::from_utf8(data)?;

        enum State {
            FirstLine,
            Header {
                got_format_version: bool,
                got_metric_units: bool,
            },
            Body {
                current_tool: Option<u32>,
                got_absolute_mode: bool,
                got_drill_mode: bool,
            },
            Done,
        }

        let mut state = State::FirstLine;

        // Map of tool index to circle diameter.
        let mut tool_diameter = HashMap::new();

        let mut holes = vec![];

        for line in data.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with(';') {
                continue;
            }

            match &mut state {
                State::FirstLine => {
                    if line != "M48" {
                        return Err(err_msg("Invalid header start line"));
                    }

                    state = State::Header {
                        got_format_version: false,
                        got_metric_units: false,
                    };
                    continue;
                }
                State::Header {
                    got_format_version,
                    got_metric_units,
                } => {
                    if line == "METRIC" {
                        *got_metric_units = true;
                        continue;
                    }

                    if line == "FMAT,2" {
                        *got_format_version = true;
                        continue;
                    }

                    if let Some(m) = TOOL_INFO_LINE.exec(line) {
                        let index = m.group_str(1).unwrap()?.parse::<u32>()?;
                        let diameter = m.group_str(2).unwrap()?.parse::<f32>()?;
                        if tool_diameter.contains_key(&index) {
                            return Err(format_err!("Duplicate tool definition: {}", line));
                        }

                        tool_diameter.insert(index, diameter);
                        continue;
                    }

                    if line == "%" {
                        if !*got_format_version || !*got_metric_units {
                            return Err(err_msg("Invalid header"));
                        }

                        state = State::Body {
                            current_tool: None,
                            got_absolute_mode: false,
                            got_drill_mode: false,
                        };
                        continue;
                    }
                }
                State::Body {
                    current_tool,
                    got_absolute_mode,
                    got_drill_mode,
                } => {
                    if line == "G90" {
                        *got_absolute_mode = true;
                        continue;
                    }

                    if line == "G05" {
                        *got_drill_mode = true;
                        continue;
                    }

                    if line == "M30" {
                        state = State::Done;
                        continue;
                    }

                    if let Some(m) = SELECT_TOOL_LINE.exec(line) {
                        let index = m.group_str(1).unwrap()?.parse::<u32>()?;
                        *current_tool = Some(index);
                        continue;
                    }

                    if let Some(m) = XY_LINE.exec(line) {
                        if !*got_absolute_mode {
                            return Err(err_msg("Only absolute mode holes supported"));
                        }

                        if !*got_drill_mode {
                            return Err(err_msg("Only drill mode holes supported"));
                        }

                        let x = m.group_str(1).unwrap()?.parse()?;
                        let y = m.group_str(2).unwrap()?.parse()?;
                        let diameter = *tool_diameter
                            .get(&current_tool.ok_or_else(|| err_msg("No tool selected"))?)
                            .ok_or_else(|| err_msg("No such tool defined"))?;

                        holes.push(DrillHole { x, y, diameter });

                        continue;
                    }
                }
                State::Done => {
                    return Err(err_msg("Lines after the end of the program"));
                }
            }

            return Err(format_err!("Unknown line: {}", line));
        }

        match state {
            State::Done => {}
            _ => {
                return Err(err_msg("Missing end of program"));
            }
        }

        Ok(Self { holes })
    }
}
