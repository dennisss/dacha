use std::{collections::HashMap, thread::current, time::Instant};

use decimal::Decimal;
use parser::Event;

#[macro_use]
extern crate regexp_macros;

mod decimal;
mod parser;

use base_error::*;

use crate::parser::*;

#[derive(Clone)]
struct Line {
    command: Command,
    params: HashMap<char, Decimal>,
    params_order: Vec<char>,
}

impl Line {
    pub fn to_string(&self) -> String {
        let mut out = self.command.to_string();

        for key in &self.params_order {
            let val = self.params.get(key).unwrap().to_string();
            out.push_str(&format!(" {}{}", *key, val));
        }

        out.push('\n');
        out
    }
}

#[derive(Clone)]
struct Command {
    group: char,
    number: Decimal,
}

impl Command {
    pub fn to_string(&self) -> String {
        format!("{}{}", self.group, self.number)
    }
}

pub fn tile_gcode(
    initial_gcode: &[u8],
    offset: (f32, f32),
    multiples: (usize, usize),
) -> Result<Vec<u8>> {
    let mut start = Instant::now();

    let mut lines: Vec<Line> = vec![];
    {
        let mut current_line: Option<Line> = None;

        let mut parser = Parser::default();
        let mut rest = initial_gcode;
        while !rest.is_empty() {
            let (e, r) = parser.parse(rest);
            rest = r;

            match e {
                Some(Event::ParseError) => return Err(err_msg("Invalid initial gcode")),
                Some(Event::Word(word)) => {
                    if let Some(line) = &mut current_line {
                        if line.params.contains_key(&word.key) {
                            return Err(err_msg("Duplicate parameter"));
                        }

                        line.params.insert(word.key, word.value);
                        line.params_order.push(word.key);
                    } else {
                        current_line = Some(Line {
                            command: Command {
                                group: word.key,
                                number: word.value,
                            },
                            params: HashMap::default(),
                            params_order: vec![],
                        });
                    }
                }
                Some(Event::EndLine) => {
                    if let Some(line) = current_line.take() {
                        lines.push(line);
                    }
                }
                None => {
                    assert!(rest.is_empty());
                }
            }
        }
    }

    let end = Instant::now();

    println!("GCode Parsed: {:?}", end - start);

    let mut out = initial_gcode.to_vec();
    // out.extend_from_slice(b"\n\n(---- BEGIN TILES ----)\n");

    let tile_count = multiples.0 * multiples.1;
    if tile_count == 0 {
        return Err(err_msg(
            "Expecting a multiple of at least 1 in each dimension",
        ));
    }

    // NOTE: tile_i == 0 is the initial tile
    for tile_i in 1..tile_count {
        let tile_y_i = tile_i / multiples.0;
        let mut tile_x_i = tile_i % multiples.0;
        // Alternate the direction of tiles to reduce the number of moves.
        if tile_y_i % 2 == 1 {
            tile_x_i = multiples.0 - tile_x_i - 1;
        }

        let x_offset = (tile_x_i as f32) * offset.0;
        let y_offset = (tile_y_i as f32) * offset.1;

        out.extend_from_slice(
            format!("\n\n(--- Tile: x: {}, y: {} ---)\n\n", tile_x_i, tile_y_i).as_bytes(),
        );

        let mut absolute_mode = false;

        for line in &lines {
            let mut line = line.clone();
            let cmd = line.command.to_string();
            match cmd.as_str() {
                "G90" => {
                    absolute_mode = true;
                }
                "G91" => {
                    absolute_mode = false;
                }
                "T1" | "M6" => {
                    // Skip toolchain ops. Assume only one tool is in use.
                    continue;
                }
                "M0" => {
                    // Get rid of pauses
                    continue;
                }
                "G21" | "G94" | "M5" | "M9" | "M3" => {
                    // Allowlisted commands which we don't need to transform.
                }
                "G0" | "G1" => {
                    // Rapid/Linear move

                    if !absolute_mode {
                        return Err(err_msg(
                            "Only programs with all absolute moves are supported",
                        ));
                    }

                    // TODO: Verify only well known params are being used.

                    if let Some(x_value) = line.params.get_mut(&'X') {
                        *x_value = *x_value + x_offset.into();
                    }

                    if let Some(y_value) = line.params.get_mut(&'Y') {
                        *y_value = *y_value + y_offset.into();
                    }
                }
                _ => {
                    return Err(format_err!("Unsupported command: {}", cmd));
                }
            }

            out.extend_from_slice(line.to_string().as_bytes());
        }
    }

    Ok(out)
}
