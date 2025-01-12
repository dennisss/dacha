use std::time::Instant;

use base_error::*;

use crate::{Command, LineBuilder, LinearMove, ProgramElement, ProgramParser, RapidMove};

struct Tiler {
    offset: (f32, f32),
    multiples: (usize, usize),
    current_layer: Vec<Command>,
    out: Vec<u8>,
}

impl Tiler {
    fn add_command(&mut self, command: Command) -> Result<()> {
        match &command {
            Command::ToolChange(_) => {
                self.flush_layer()?;
            }
            Command::Stop(_) => {
                self.flush_layer();

                let mut line = LineBuilder::default();
                line.add(&command);
                self.out.extend_from_slice(line.to_string().as_bytes());

                // Do not append.
                return Ok(());
            }

            Command::SetToAbsoluteMode(_)
            | Command::RapidMove(_)
            | Command::LinearMove(_)
            | Command::SetUnitsToMillimeters(_)
            | Command::CutterCompensationOff(_)
            | Command::Workspace1Coordinates(_)
            | Command::G80(_)
            | Command::FeedRateUnitsPerMinute(_)
            | Command::SpindleOff(_)
            | Command::SpindleOnClockwise(_)
            | Command::SpindleOnCounterClockwise(_) => {
                //
            }

            Command::SetToRelativeMode(_) => {
                return Err(err_msg("Relative mode geometry not supported"));
            }

            _ => {
                return Err(format_err!("Unsupported command: {:?}", command));
            }
        }

        self.current_layer.push(command);

        Ok(())
    }

    fn flush_layer(&mut self) -> Result<()> {
        if self.current_layer.is_empty() {
            return Ok(());
        }

        let tile_count = self.multiples.0 * self.multiples.1;
        if tile_count == 0 {
            return Err(err_msg(
                "Expecting a multiple of at least 1 in each dimension",
            ));
        }

        // NOTE: tile_i == 0 is the initial tile
        for tile_i in 0..tile_count {
            let tile_y_i = tile_i / self.multiples.0;
            let mut tile_x_i = tile_i % self.multiples.0;
            // Alternate the direction of tiles to reduce the number of moves.
            if tile_y_i % 2 == 1 {
                tile_x_i = self.multiples.0 - tile_x_i - 1;
            }

            let x_offset = (tile_x_i as f32) * self.offset.0;
            let y_offset = (tile_y_i as f32) * self.offset.1;

            self.out.extend_from_slice(
                format!("\n\n(--- Tile: x: {}, y: {} ---)\n\n", tile_x_i, tile_y_i).as_bytes(),
            );

            for command in &self.current_layer {
                let mut command = command.clone();
                match &mut command {
                    Command::LinearMove(LinearMove { inner })
                    | Command::RapidMove(RapidMove { inner }) => {
                        if let Some(x) = &mut inner.x {
                            *x = *x + x_offset.into();
                        }

                        if let Some(y) = &mut inner.y {
                            *y = *y + y_offset.into();
                        }
                    }
                    _ => {}
                }

                let mut line = LineBuilder::default();
                line.add(&command);
                self.out.extend_from_slice(line.to_string().as_bytes());
            }
        }

        self.current_layer.clear();

        Ok(())
    }

    fn finish(mut self) -> Result<Vec<u8>> {
        self.flush_layer();
        Ok(self.out)
    }
}

/// Tiles a set of gcode file on the xy plane.
///
/// - Each 'layer' is tiled separately such that all copies of one layer are
///   executed before proceeding to the next layer.
/// - We define a layer is a contiguous segment of gcode starting with a tool
///   change or stop command.
///
/// TODO: For 3d printing, this needs to be 'z' layer aware
pub fn tile_gcode(
    initial_gcode: &[u8],
    offset: (f32, f32),
    multiples: (usize, usize),
) -> Result<Vec<u8>> {
    let mut start = Instant::now();

    let mut parser = ProgramParser::default();

    let mut tiler = Tiler {
        offset,
        multiples,
        current_layer: vec![],
        out: vec![],
    };

    {
        let mut input = initial_gcode;
        let mut out = vec![];

        while !input.is_empty() {
            out.clear();
            let n = parser.parse_line(input, true, &mut out);
            input = &input[n..];

            if out.is_empty() {
                continue;
            }

            for el in out.drain(..) {
                match el {
                    ProgramElement::Thumbnail(_) => {}
                    ProgramElement::Command(cmd) => {
                        // NOTE: We split lines to individual commands since we don't currently
                        // support any commands that only affect the current
                        // line.

                        tiler.add_command(cmd)?;
                    }
                    ProgramElement::Metadata { key, value } => {}
                    ProgramElement::Error(e) => {
                        return Err(format_err!("Error in parsing the input gcode: {}", e));
                    }
                    ProgramElement::EndOfLine => {}
                }
            }
        }
    }

    tiler.finish()
}
