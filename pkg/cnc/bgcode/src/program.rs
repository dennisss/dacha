use base_error::*;

use crate::decoder::{Decoder, Event};
use crate::FILE_MAGIC;

/// A program parser which can process either a binary or text gcode file.
#[derive(Default)]
pub struct ProgramParser {
    state: State,
}

enum State {
    StartOfFile,
    Text(gcode::ProgramParser),
    Binary(BinaryState),
}

impl Default for State {
    fn default() -> Self {
        Self::StartOfFile
    }
}

struct BinaryState {
    decoder: Decoder,

    /// Stores raw decoded gcode returned by the 'decoder'.
    buffer: Vec<u8>,

    buffer_offset: usize,

    buffer_end: usize,

    buffer_done: bool,

    gcode_parser: Option<gcode::ProgramParser>,
}

impl ProgramParser {
    #[must_use]
    pub fn parse_line(
        &mut self,
        input: &[u8],
        end_of_input: bool,
        out: &mut Vec<gcode::ProgramElement>,
    ) -> Result<usize> {
        match &mut self.state {
            State::StartOfFile => {
                if input.len() < FILE_MAGIC.len() {
                    return Err(err_msg("First block too small to detect file magic."));
                }

                if &input[..FILE_MAGIC.len()] == FILE_MAGIC {
                    self.state = State::Binary(BinaryState {
                        decoder: Decoder::new(),
                        buffer: vec![0u8; 4096],
                        buffer_offset: 0,
                        buffer_end: 0,
                        buffer_done: false,
                        gcode_parser: None,
                    });
                } else {
                    self.state = State::Text(gcode::ProgramParser::default());
                }

                self.parse_line(input, end_of_input, out)
            }
            State::Text(parser) => Ok(parser.parse_line(input, end_of_input, out)),
            State::Binary(state) => Self::parse_binary_line(state, input, end_of_input, out),
        }
    }

    fn parse_binary_line(
        state: &mut BinaryState,
        input: &[u8],
        end_of_input: bool,
        out: &mut Vec<gcode::ProgramElement>,
    ) -> Result<usize> {
        let mut input_read = 0;

        loop {
            if let Some(gcode_parser) = &mut state.gcode_parser {
                loop {
                    let n = gcode_parser.parse_line(
                        &state.buffer[state.buffer_offset..state.buffer_end],
                        state.buffer_done,
                        out,
                    );
                    state.buffer_offset += n;

                    if let Some(gcode::ProgramElement::EndOfLine) = out.last() {
                        return Ok(input_read);
                    }

                    if n == 0 {
                        break;
                    }
                }

                if state.buffer_done {
                    state.gcode_parser = None;
                }
            }

            // Step 1: If we have data left in the buffer, parse it out.

            let progress =
                state
                    .decoder
                    .update(&input[input_read..], end_of_input, &mut state.buffer)?;
            input_read += progress.input_read;
            state.buffer_offset = 0;
            state.buffer_end = progress.output_written;

            match progress.event {
                Event::Pending => {
                    // Need more input data.
                    break;
                }
                Event::Metadata { typ, data } => {
                    if let Err(e) = Self::parse_metadata_block(&data, out) {
                        out.push(gcode::ProgramElement::Error(e));
                    }

                    out.push(gcode::ProgramElement::EndOfLine);
                }
                Event::Thumbnail { params, data } => {
                    out.push(gcode::ProgramElement::Thumbnail(gcode::ProgramThumbnail {
                        data: data.into(),
                        width: params.width as usize,
                        height: params.height as usize,
                    }));
                    out.push(gcode::ProgramElement::EndOfLine);
                    break;
                }
                Event::GCode => {
                    if state.gcode_parser.is_none() {
                        state.gcode_parser = Some(gcode::ProgramParser::default());
                    }

                    // Loop and parse some of the code.
                    continue;
                }
                Event::GCodeEnd => {
                    state.buffer_done = true;
                    continue;
                }
            }
        }

        Ok(input_read)
    }

    // NOTE: The assumption is that the metadata blocks are always in ini format.
    fn parse_metadata_block(data: &[u8], out: &mut Vec<gcode::ProgramElement>) -> Result<()> {
        let data = std::str::from_utf8(data)?;

        for line in data.lines() {
            let mut line = line.trim();
            if line.is_empty() {
                continue;
            }

            let (key, value) = line
                .split_once('=')
                .ok_or_else(|| err_msg("Missing equals sign in ini line"))?;

            out.push(gcode::ProgramElement::Metadata {
                key: key.trim().to_string(),
                value: value.trim().to_string(),
            });
        }

        Ok(())
    }
}
