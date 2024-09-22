use base_error::*;
use compression::transform::*;

use crate::constants::*;

pub struct Decoder {
    packed: bool,
    no_spaces: bool,
    state: State,
}

impl Decoder {
    pub fn new() -> Self {
        Self {
            packed: false,
            no_spaces: false,
            state: State::Start,
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum State {
    Start,

    /// The last input byte was a 0xFF.
    Escaped,

    /// The last two input bytes were [0xFF, 0xFF].
    /// The next byte will be a command byte.
    Command,

    /// The next byte in the input should be interprated as a literal byte and
    /// written as is to the output.
    ///
    /// Returns to 'Start' once done.
    Literal,

    /// Same as Literal, except transition to the Packed state after consuming
    /// one literal byte.
    LiteralThenPacked(u8),

    /// Waiting to write the given byte to the output buffer.
    Packed(u8),
}

impl Transform for Decoder {
    fn update(
        &mut self,
        input: &[u8],
        end_of_input: bool,
        mut output: &mut [u8],
    ) -> Result<TransformProgress> {
        let mut input_read = 0;
        let mut output_written = 0;

        // NOTE: This will always consume either the input, output, or both buffers.
        while input_read < input.len() && output_written < output.len() {
            let input_byte = input[input_read];

            self.state = match self.state {
                State::Start => {
                    if input_byte == ESCAPE_CODE {
                        input_read += 1;
                        State::Escaped
                    } else if self.packed {
                        let first_code = input_byte & 0b1111;
                        let second_code = input_byte >> 4;
                        input_read += 1;

                        let lut = {
                            if self.no_spaces {
                                &LOOKUP_4_TO_8_BIT_NO_SPACES
                            } else {
                                &LOOKUP_4_TO_8_BIT
                            }
                        };

                        if first_code == 0b1111 {
                            State::LiteralThenPacked(lut[second_code as usize])
                        } else if second_code == 0b1111 {
                            output[output_written] = lut[first_code as usize];
                            output_written += 1;

                            State::Literal
                        } else {
                            output[output_written] = lut[first_code as usize];
                            output_written += 1;

                            State::Packed(lut[second_code as usize])
                        }
                    } else {
                        // Normal. Same as literal state.
                        output[output_written] = input_byte;
                        output_written += 1;
                        input_read += 1;
                        State::Start
                    }
                }
                State::Escaped => {
                    if input_byte == ESCAPE_CODE {
                        // Got 2 0xFF bytes in a row. The next byte will be a command.
                        input_read += 1;
                        State::Command
                    } else if self.packed {
                        // Had a prior 0xFF without a 0xFF following it. So we have expect 2 literal
                        // bytes following it.

                        // First byte.
                        output[output_written] = input_byte;
                        output_written += 1;
                        input_read += 1;

                        // Second byte (handled in next loop iteration)
                        State::Literal
                    } else {
                        output[output_written] = ESCAPE_CODE;
                        output_written += 1;
                        State::Start
                    }
                }
                State::Literal => {
                    output[output_written] = input_byte;
                    output_written += 1;
                    input_read += 1;
                    State::Start
                }
                State::LiteralThenPacked(v) => {
                    output[output_written] = input_byte;
                    output_written += 1;
                    input_read += 1;

                    State::Packed(v)
                }
                State::Packed(v) => {
                    output[output_written] = v;
                    output_written += 1;
                    State::Start
                }
                State::Command => {
                    let cmd = Command::from_value(input_byte)?;
                    input_read += 1;

                    match cmd {
                        Command::None => {}
                        Command::EnablePacking => {
                            self.packed = true;
                        }
                        Command::DisablePacking => {
                            self.packed = false;
                        }
                        Command::ResetAll => {
                            self.packed = false;
                            self.no_spaces = false;
                        }
                        Command::QueryConfig => {}
                        Command::EnableNoSpaces => {
                            self.no_spaces = true;
                        }
                        Command::DisableNoSpaces => {
                            self.no_spaces = false;
                        }
                    }

                    State::Start
                }
            };
        }

        // The above loop may stop if we ran out of input but we still have extra output
        // space, so write any buffered output bytes if we can.
        // TODO: Add unit testing for this.
        if output_written < output.len() {
            if let State::Packed(v) = self.state {
                output[output_written] = v;
                output_written += 1;
                self.state = State::Start;
            }
        }

        // We are done once:
        // 1) We have consumed all inputs.
        // 2) All pending output has been written to the output buffer.
        let mut done = false;
        if input_read == input.len() && end_of_input {
            done = match self.state {
                State::Start => true,
                State::Escaped => {
                    if self.packed {
                        return Err(err_msg("Too few bytes in input. Expected 2 more literals."));
                    }

                    if output_written < output.len() {
                        output[output_written] = ESCAPE_CODE;
                        output_written += 1;
                        self.state = State::Start;
                        true
                    } else {
                        false
                    }
                }
                State::Packed(v) => false,
                State::Command | State::Literal | State::LiteralThenPacked(_) => {
                    return Err(format_err!(
                        "Too few bytes in input. State: {:?}",
                        self.state
                    ));
                }
            };
        }

        Ok(TransformProgress {
            input_read,
            output_written,
            done,
            event: (),
        })
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn decoder_works() {
        // (encoded, decoded) pairs
        let test_cases: &'static [(&'static [u8], &'static [u8])] = &[
            (b"hello world!", b"hello world!"),
            (b"hello\xFF", b"hello\xFF"),
            // 'hel',  [no-op], 'lo'
            (b"hel\xFF\xFF\x00lo", b"hello"),
            // [enable packing]
            (b"\xFF\xFF\xFB", b""),
            // [enable packing], [disable packing]
            (b"\xFF\xFF\xFB\xFF\xFF\xFA", b""),
            // [enable packing], [literal bytes: 1, 2]
            (b"\xFF\xFF\xFB\xFF\x01\x02", b"\x01\x02"),
            (b"\xFF\xFF\xFB\x0D", b"G0"),
            (b"\xFF\xFF\xFB\xFD!", b"G!"),
            (b"\xFF\xFF\xFB\xDF!", b"!G"),
        ];

        for (encoded, decoded) in test_cases.iter().cloned() {
            let mut out = vec![];
            transform_to_vec(Decoder::new(), encoded, &mut out).unwrap();
            assert_eq!(&out, decoded);
        }
    }
}
