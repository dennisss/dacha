use base_error::*;
use common::bytes::Bytes;
use common::format::format_bytes;
use gcode_decimal::Decimal;

use crate::command::{Command, CommandCodec, CommandWord, LineParameters};
use crate::parser::*;

/*
GCode parsing variations:

- Marlin
    - Need to assume that the first word is the command (to avoid marking 'P' as a parameter)
    - 'T' and 'P' are commands and not parameters
    - Up to one command per line
- gRBL / Smoothieware
    - Commands are just 'M' and 'G'
    - 'M6' is used for tool change.
    - Multiple commands can appear on one line



*/

#[derive(Debug)]
pub enum ProgramElement {
    Thumbnail(ProgramThumbnail),

    Command(Command),

    Error(Error),

    /// NOTE: If present, this will always be the last element in the 'out'
    /// argument of parse_line.
    EndOfLine,
}

#[derive(Debug)]
pub struct ProgramThumbnail {
    pub data: Bytes,
    pub width: usize,
    pub height: usize,
}

/// NOTE: Call finish() after all lines are parsed to finalize any partial
/// state.
#[derive(Default)]
pub struct ProgramParser {
    parser: Parser,
    state: State,
}

#[derive(Default)]
struct State {
    line_state: LineState,

    /// If Some, then we have partially parsed a thumbnail.
    partial_thumbnail: Option<PartialThumbnail>,

    /// Last command in the motion modal group.
    ///
    /// TODO: Implement proper support for generic modal groups.
    last_motion_command: Option<CommandWord>,
}

#[derive(Default)]
struct LineState {
    event_index: usize,

    // G and M codes seen in the line.
    // These are the only two letters that we allow to be repeated across words.
    command_codes: Vec<CommandWord>,

    // All other words seen on the line that aren't G or M style.
    params: LineParameters,

    first_word: Option<Word>,

    error: Option<Error>,

    /// Up to the first 128 bytes of the current line if the line started in a
    /// previous parse_line call.
    line_start: Vec<u8>,
}

impl LineState {
    /// A way to reset to LineState::default() while re-using the memory
    /// buffers.
    fn reset(&mut self) {
        self.event_index = 0;
        self.command_codes.clear();
        self.params.clear();
        self.first_word = None;
        self.error = None;
        self.line_start.clear();
    }
}

struct PartialThumbnail {
    start_tag: String,
    width: usize,
    height: usize,
    size: usize,
    data_base64: String,
}

impl ProgramParser {
    /// Parses up to one line worth of data from a program.
    ///
    /// Note that this will always succeed:
    /// - If a line has an error, it will be indicated by an emitted
    ///   ProgramElement::Error.
    /// - Future calls to this function will skip ahead to the start of the next
    ///   line.
    #[must_use]
    pub fn parse_line(
        &mut self,
        input: &[u8],
        end_of_input: bool,
        out: &mut Vec<ProgramElement>,
    ) -> usize {
        // TODO: Support enforcing only one command per line for marlin style printers.

        let mut line_done = false;

        // TODO: Don't assume end of input here.
        let mut iter = self.parser.iter(&input[..], end_of_input);
        while let Some(e) = iter.next() {
            match e {
                Event::ProgramDelimiter => {
                    // TODO: Verify this is the first non-empty line or the
                    // last non-empty line in the file.
                }
                Event::LineNumber(_) => {}
                Event::Comment(data, is_semi_comment) => {
                    if is_semi_comment && self.state.line_state.event_index == 0 {
                        // TODO: Verify that we are using consecutive lines for the comment.
                        if let Err(e) = self.state.parse_thumbnail_comment(data, out) {
                            self.state.line_state.error.get_or_insert(e);
                        }
                    }
                }
                Event::ParseError(e) => {
                    self.state
                        .line_state
                        .error
                        .get_or_insert(format_err!("GCode parsing error: {:?}", e));
                }
                Event::EndLine => {
                    // TODO: Finalize the line if there was no error.

                    if self.state.line_state.error.is_none() {
                        if let Err(e) = self.state.finalize_line(out) {
                            self.state.line_state.error.get_or_insert(e);
                        }
                    }

                    if let Some(e) = self.state.line_state.error.take() {
                        let input_read = input.len() - iter.remaining().len();
                        self.state.cache_line_prefix(&input[..input_read]);

                        // TODO: Also escape quotes when formatting the line.
                        let e = format_err!(
                            "Line: \"{}\": {}",
                            format_bytes(&self.state.line_state.line_start),
                            e
                        );

                        out.clear();
                        out.push(ProgramElement::Error(e));
                    }

                    out.push(ProgramElement::EndOfLine);

                    self.state.line_state.reset();
                    line_done = true;

                    break;
                }
                Event::Word(w) => {
                    if let Err(e) = self.state.handle_word(w) {
                        self.state.line_state.error.get_or_insert(e);
                    }
                }
            }

            self.state.line_state.event_index += 1;
        }

        let input_read = input.len() - iter.remaining().len();

        if input_read == input.len() && end_of_input {
            if self.state.partial_thumbnail.is_some() {
                out.push(ProgramElement::Error(err_msg(
                    "Hit the end of the file before parsing the entire thumbnail",
                )));
                out.push(ProgramElement::EndOfLine);
            }
        }

        if !line_done {
            self.state.cache_line_prefix(&input[..input_read]);
        }

        input_read
    }
}

impl State {
    fn cache_line_prefix(&mut self, data: &[u8]) {
        let n = core::cmp::min(128 - self.line_state.line_start.len(), data.len());
        self.line_state.line_start.extend_from_slice(data);
    }

    fn handle_word(&mut self, w: Word) -> Result<()> {
        if (w.key == 'G' || w.key == 'M') && w.value != WordValue::Empty {
            self.line_state
                .command_codes
                .push(CommandWord::from_word(&w)?);
        } else {
            if self.line_state.first_word.is_none() {
                self.line_state.first_word = Some(w.clone());
            }

            self.line_state.params.add_param(w.key, w.value)?;
        }

        Ok(())
    }

    fn finalize_line(&mut self, out: &mut Vec<ProgramElement>) -> Result<()> {
        let mut commands = vec![];

        // TODO: Sort commands like 'G53' so that are referenced before other things on
        // the line. Also want things like spindle changes to precede movements.

        for command_code in self.line_state.command_codes.drain(..) {
            if command_code == command_word!("G0")
                || command_code == command_word!("G1")
                || command_code == command_word!("G2")
                || command_code == command_word!("G3")
            {
                self.last_motion_command = Some(command_code.clone());
            }

            commands.push(Command::from_command_words(
                command_code,
                &mut self.line_state.params,
            )?);
        }

        if let Some(motion_command) = &self.last_motion_command {
            if self.line_state.params.peek_has_remaining('X')
                || self.line_state.params.peek_has_remaining('Y')
                || self.line_state.params.peek_has_remaining('Z')
            {
                commands.push(Command::from_command_words(
                    motion_command.clone(),
                    &mut self.line_state.params,
                )?);
            }
        }

        if let Some(word) = self.line_state.first_word.take() {
            if word.key == 'T' && self.line_state.params.peek_has_remaining('T') {
                let cmd = CommandWord::from_word(&word)?;

                commands.push(Command::SelectTool(
                    crate::command::SelectTool::from_command_words(
                        cmd,
                        &mut self.line_state.params,
                    )?,
                ));
            }

            if word.key == 'P' && self.line_state.params.peek_has_remaining('P') {
                let cmd = CommandWord::from_word(&word)?;

                commands.push(Command::ParkTool(
                    crate::command::ParkTool::from_command_words(cmd, &mut self.line_state.params)?,
                ));
            }
        }

        if !self.line_state.params.is_empty() {
            return Err(format_err!(
                "Not all parameters parsed. Remaining: {:?}",
                self.line_state.params.debug_remaining_unparsed()
            ));
        }

        // TODO: Inline this above.
        for command in commands {
            out.push(ProgramElement::Command(command));
        }

        Ok(())
    }

    /*
    Typically thumbnails are stored in the gcode files with lines that look like the following:
    ;
    ; thumbnail begin 160x120 16996
    ; iVBORw0KGgoAAAANSUhEUgAAAKAAAAB4CAYAAAB1ovlvAAAxkUlEQVR4Ae2d+ZOc1Xnv33AL55Zvqs
    ; D32nHZufc6DoTYTqIYkxAMCNAyo32075p9RgsSWhBCbLIQq5DYDWYxqyWM2IwNGNsylRjjuJzEqSQ/
    ....
    ; pSr/zJv+nNG3eebM2d5WS5ZEfng06u63T5/le579PKcaHByse3p66uuuu24SrV27th4ZGalnzZpVz5
    ; thumbnail end
    ;
    */
    fn parse_thumbnail_comment(
        &mut self,
        data: &[u8],
        out: &mut Vec<ProgramElement>,
    ) -> Result<()> {
        let data = core::str::from_utf8(data)?.trim();
        if data.is_empty() {
            return Ok(());
        }

        let parts = data.split_ascii_whitespace().collect::<Vec<_>>();

        if self.partial_thumbnail.is_none()
            && (parts[0] == "thumbnail"
                || parts[0] == "thumbnail_QOI"
                || parts[0] == "thumbnail_JPG")
            && parts.len() >= 2
            && parts[1] == "begin"
        {
            if parts.len() < 4 {
                return Err(err_msg(
                    "Expected at least 4 fields in thumbnail start line",
                ));
            }

            let (width_str, height_str) = parts[2]
                .split_once('x')
                .ok_or_else(|| err_msg("Invalid image dimensions format"))?;
            let width = width_str.parse::<usize>()?;
            let height = height_str.parse::<usize>()?;

            let size = parts[3].parse::<usize>()?;

            self.partial_thumbnail = Some(PartialThumbnail {
                start_tag: parts[0].to_string(),
                width,
                height,
                size,
                data_base64: String::new(),
            });
            return Ok(());
        }

        let mut thumb = match self.partial_thumbnail.take() {
            Some(v) => v,
            None => return Ok(()),
        };

        if parts.len() >= 2 && parts[0] == &thumb.start_tag && parts[1] == "end" {
            if thumb.data_base64.len() != thumb.size {
                return Err(err_msg("Not enough data was parsed for the thumbnail"));
            }

            let data = base_radix::base64_decode(&thumb.data_base64)?.into();

            out.push(ProgramElement::Thumbnail(ProgramThumbnail {
                data,
                width: thumb.width,
                height: thumb.height,
            }));
            return Ok(());
        }

        // TODO: Don't allow overflowing the size.
        thumb.data_base64.push_str(data);

        self.partial_thumbnail = Some(thumb);

        Ok(())
    }
}
