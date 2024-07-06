use std::{collections::HashMap, time::Instant};

use decimal::Decimal;

#[macro_use]
extern crate regexp_macros;

mod decimal;
mod hints;
mod line;
mod parser;

use base_error::*;

pub use crate::line::*;
pub use crate::parser::*;

pub const MAX_STANDARD_LINE_LENGTH: usize = 256;

pub fn tile_gcode(
    initial_gcode: &[u8],
    offset: (f32, f32),
    multiples: (usize, usize),
) -> Result<Vec<u8>> {
    let mut start = Instant::now();

    let mut lines: Vec<Line> = vec![];
    {
        let mut current_line = LineBuilder::new();

        let mut parser = Parser::new();
        let mut iter = parser.iter(initial_gcode, true);

        while let Some(e) = iter.next() {
            match e {
                Event::LineNumber(_) => {}
                Event::ParseError(_) => return Err(err_msg("Invalid initial gcode")),
                Event::Word(word) => {
                    current_line.add_word(word)?;
                }
                Event::EndLine => {
                    let mut l = LineBuilder::new();
                    core::mem::swap(&mut l, &mut current_line);

                    if let Some(line) = l.finish() {
                        lines.push(line);
                    }
                }
                Event::Comment(..) => {}
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
            let cmd = line.command().to_string();
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

                    if let Some(x_value) = line.param_mut('X') {
                        let x_value = match x_value {
                            WordValue::RealValue(v) => v,
                            _ => return Err(err_msg("X is not a number")),
                        };

                        *x_value = *x_value + x_offset.into();
                    }

                    if let Some(y_value) = line.param_mut('Y') {
                        let y_value = match y_value {
                            WordValue::RealValue(v) => v,
                            _ => return Err(err_msg("Y is not a number")),
                        };

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
