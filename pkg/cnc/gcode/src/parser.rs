use base_error::*;

use crate::decimal::Decimal;

/// Maximum of 2048 bytes per line if the larger line ending of '\r\n' is used.
/// 3d printer slicing software likes to put very large comments into the files.
const MAX_LINE_LENGTH: usize = 2048 - 2;

#[derive(Debug, PartialEq, Clone)]
pub struct Word {
    /// ASCII uppercase letter identifying this word.
    pub key: char,

    pub value: WordValue,
}

#[derive(Debug, PartialEq, Clone)]
pub enum WordValue {
    RealValue(Decimal),
    QuotedString(Vec<u8>),
    UnquotedString(Vec<u8>),
    Empty,
}

impl WordValue {
    pub fn to_string(&self) -> String {
        match self {
            WordValue::RealValue(v) => v.to_string(),
            WordValue::QuotedString(_) => todo!(),
            WordValue::UnquotedString(_) => todo!(),
            WordValue::Empty => String::new(),
        }
    }

    pub fn to_f32(&self) -> Result<f32> {
        match self {
            Self::RealValue(v) => Ok(v.to_f32()),
            _ => Err(err_msg("Not a number")),
        }
    }
}

#[derive(Debug, PartialEq, Clone)]
pub enum Event<'a> {
    LineNumber(usize),
    Word(Word),

    /// Emitted at the end of each line. Either due to hitting a line ending
    /// (\r or \n) or the end of the input stream.
    EndLine,
    ParseError(ParseErrorKind),
    Comment(&'a [u8], bool),
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum ParseErrorKind {
    LineTooLong,
    InvalidComponentKey,
    InvalidLineNumber,
    UnterminatedString,
    UnterminatedComment,
}

pub struct Parser {
    state: ParserState,

    /// Absolute byte position in the input stream.
    /// (equal to the number of bytes consumed)
    offset: usize,

    /// Number of the current line we are parsing. Starts at 1.
    line_number: usize,

    /// Start position of the current line.
    line_offset: usize,

    buffer: Vec<u8>,

    word_key: u8,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum ParserState {
    StartOfLine,

    /// We are ignoring all bytes until we reach the next line.
    SkipLine,

    /// We got a '\r' character. We may optionally consume a single '\n'
    /// character without emitting another event.
    GotCarriageReturn,

    /// We are currently reading the line number into 'buffer'
    InLineNumber,

    StartOfLineComponent,

    /// We are currently reading the value of a word. No non-whitespace values
    /// have been seen yet.
    InWordStart,

    InWordRegularValue,

    /// We have seen a starting quote and
    InWordQuotedValue,

    InParenComment,

    /// We are currently reading a semi-colon delimited comment into 'buffer'.
    InSemiComment,
}

impl Parser {
    /// NOTE: Only complete lines can be passed
    pub fn new() -> Self {
        let mut buffer = vec![];
        buffer.reserve_exact(256);

        Self {
            state: ParserState::StartOfLine,
            line_number: 1,
            line_offset: 0,
            buffer,
            word_key: 0,
            offset: 0,
        }
    }

    pub fn iter<'a: 'b, 'b>(
        &'a mut self,
        mut data: &'b [u8],
        end_of_input: bool,
    ) -> ParserIterator<'a, 'b> {
        ParserIterator {
            parser: self,
            remaining: data,
            end_of_input,
        }
    }

    /// Parses more data from the input gcode stream.
    ///
    ///
    /// Passing an empty 'data' parameter implies that all inputs have been
    /// consumed and none will be received in the future. next() must be called
    /// exactly once with an empty 'data' parameter to flush any partial parsing
    /// that was done.
    ///
    /// Returns the next event emitted by the parser and the number of bytes
    /// consumed in the input data. Each call of next() will either emit an
    /// event, consume all of the input, or both. Future calls to next() should
    /// pass in all unconsumed input data.
    pub fn next<'a>(
        &'a mut self,
        mut data: &[u8],
        end_of_input: bool,
    ) -> (Option<Event<'a>>, usize) {
        let mut event = None;

        let mut i = 0;

        // Loop through the data
        // - At most one character is consumed on each loop iteration.
        // - The loop will break once an event is emitted.
        loop {
            let c = {
                if i < data.len() {
                    data[i]
                }
                // When we hit the end of inputs, consume one extra '\n' character to terminate any
                // partial parsing state.
                else if end_of_input && i == data.len() && self.state != ParserState::StartOfLine
                {
                    b'\n'
                } else {
                    break;
                }
            };

            // Check if adding this character to the current line would make it exceed the
            // max length.
            // NOTE: Currently line endings aren't counted toward the length limit.
            if self.state != ParserState::SkipLine && self.state != ParserState::GotCarriageReturn {
                let line_length = (self.offset + i) - self.line_offset;
                if line_length + 1 > MAX_LINE_LENGTH {
                    event = Some(Event::ParseError(ParseErrorKind::LineTooLong));
                    self.state = ParserState::SkipLine;
                    break;
                }
            }

            // assert!(last_size != data.len());
            // last_size = data.len();

            // match

            /*
            TODO: Limit to 256 characters per line.

            If it is too much

            If we haven't fired an Error, fire one once for the line.

            */

            match self.state {
                ParserState::StartOfLine => {
                    // Skip whitespace
                    if Self::is_inline_whitespace(c) {
                        i += 1;
                        continue;
                    }

                    if c == b'N' || c == b'n' {
                        i += 1;
                        self.state = ParserState::InLineNumber;
                        self.buffer.clear();
                        continue;
                    }

                    self.state = ParserState::StartOfLineComponent;
                }

                ParserState::SkipLine => {
                    if c == b'\n' {
                        i += 1;
                        event = Some(Event::EndLine);
                        self.line_number += 1;
                        self.line_offset = self.offset + i;
                        self.state = ParserState::StartOfLine;
                        break;
                    }

                    if c == b'\r' {
                        i += 1;
                        self.state = ParserState::GotCarriageReturn;
                        continue;
                    }

                    i += 1;
                }
                ParserState::GotCarriageReturn => {
                    if c == b'\n' {
                        i += 1;
                    }

                    self.line_number += 1;
                    self.line_offset = self.offset + i;
                    event = Some(Event::EndLine);
                    self.state = ParserState::StartOfLine;
                    break;
                }
                ParserState::StartOfLineComponent => {
                    if Self::is_inline_whitespace(c) {
                        i += 1;
                        continue;
                    }

                    if c == b';' {
                        i += 1;
                        self.state = ParserState::InSemiComment;
                        self.buffer.clear();
                        continue;
                    }

                    if c == b'(' {
                        i += 1;
                        self.state = ParserState::InParenComment;
                        self.buffer.clear();
                        continue;
                    }

                    let c = c.to_ascii_uppercase();
                    if c.is_ascii_alphabetic() && c != b'N' {
                        i += 1;
                        self.state = ParserState::InWordStart;
                        self.word_key = c;
                        continue;
                    }

                    if c == b'\n' || c == b'\r' {
                        self.state = ParserState::SkipLine;
                        continue;
                    }

                    i += 1;
                    self.state = ParserState::SkipLine;
                    event = Some(Event::ParseError(ParseErrorKind::InvalidComponentKey));
                    break;
                }

                ParserState::InLineNumber => {
                    // Skip whitespace
                    if Self::is_inline_whitespace(c) {
                        i += 1;
                        continue;
                    }

                    if !c.is_ascii_digit() {
                        let v = match core::str::from_utf8(&self.buffer)
                            .ok()
                            .and_then(|s| s.parse::<usize>().ok())
                        {
                            Some(v) => v,
                            None => {
                                self.state = ParserState::SkipLine;
                                event = Some(Event::ParseError(ParseErrorKind::InvalidLineNumber));
                                break;
                            }
                        };

                        event = Some(Event::LineNumber(v));
                        self.state = ParserState::StartOfLineComponent;
                        break;
                    }

                    // Not allowed to have more than 5 digits.
                    if self.buffer.len() == 5 {
                        i += 1;
                        self.state = ParserState::SkipLine;
                        event = Some(Event::ParseError(ParseErrorKind::InvalidLineNumber));
                        break;
                    }

                    self.buffer.push(c);
                    i += 1;
                }
                ParserState::InWordStart => {
                    // Skip whitespace
                    if Self::is_inline_whitespace(c) {
                        i += 1;
                        continue;
                    }

                    if Self::is_word_value_terminator(c) {
                        event = Some(Event::Word(Word {
                            key: self.word_key as char,
                            value: WordValue::Empty,
                        }));
                        self.state = ParserState::StartOfLineComponent;
                        break;
                    }

                    if c == b'"' {
                        i += 1;
                        self.state = ParserState::InWordQuotedValue;
                        self.buffer.clear();
                        continue;
                    }

                    self.buffer.clear();
                    self.buffer.push(c);
                    self.state = ParserState::InWordRegularValue;
                    i += 1;
                }
                ParserState::InWordRegularValue => {
                    // Skip whitespace
                    if Self::is_inline_whitespace(c) {
                        i += 1;
                        continue;
                    }

                    if Self::is_word_value_terminator(c) {
                        let value = {
                            if let Some(v) = Decimal::parse_complete(&self.buffer) {
                                WordValue::RealValue(v)
                            } else {
                                WordValue::UnquotedString(self.buffer.clone())
                            }
                        };

                        event = Some(Event::Word(Word {
                            key: self.word_key as char,
                            value,
                        }));
                        self.state = ParserState::StartOfLineComponent;
                        break;
                    }

                    self.buffer.push(c);
                    i += 1;
                }
                ParserState::InWordQuotedValue => {
                    if c == b'\r' || c == b'\n' {
                        self.state = ParserState::SkipLine;
                        event = Some(Event::ParseError(ParseErrorKind::UnterminatedString));
                        break;
                    }

                    // TODO: Handle escaped strings.

                    if c == b'"' {
                        i += 1;
                        self.state = ParserState::StartOfLineComponent;
                        event = Some(Event::Word(Word {
                            key: self.word_key as char,
                            value: WordValue::QuotedString(self.buffer.clone()),
                        }));

                        break;
                    }

                    self.buffer.push(c);
                    i += 1;
                }

                ParserState::InParenComment => {
                    // - It is an error to get a '(' while in a comment.
                    // - It is also an error to hit the end of a line without closing the comment.
                    if c == b'(' || c == b'\n' || c == b'\r' {
                        self.state = ParserState::SkipLine;
                        event = Some(Event::ParseError(ParseErrorKind::UnterminatedComment));
                        break;
                    }

                    // End of comment.
                    if c == b')' {
                        i += 1;
                        self.state = ParserState::StartOfLineComponent;
                        event = Some(Event::Comment(&self.buffer[..], false));
                        break;
                    }

                    self.buffer.push(c);
                    i += 1;
                }
                ParserState::InSemiComment => {
                    if c == b'\r' || c == b'\n' {
                        self.state = ParserState::StartOfLineComponent;
                        event = Some(Event::Comment(&self.buffer[..], true));
                        break;
                    }

                    self.buffer.push(c);
                    i += 1;
                }
            }
        }

        // Suppress the extra tokens appended for the end_of_input case.
        if i > data.len() {
            i = data.len();
        }

        self.offset += i;

        (event, i)
    }

    fn is_inline_whitespace(c: u8) -> bool {
        c == b' ' || c == b'\t'
    }

    fn is_word_value_terminator(c: u8) -> bool {
        c == b'\r' || c == b'\n' || c == b';' || c == b'(' || c.is_ascii_alphabetic()
    }

    /// Current line number (incremented when an EndLine event is emitted)
    pub fn current_line_number(&self) -> usize {
        self.line_number
    }

    // pub fn offset(&self) -> usize {
    //     self.data.len() - self.remaining.len()
    // }
}

pub struct ParserIterator<'a, 'b> {
    parser: &'a mut Parser,
    remaining: &'b [u8],
    end_of_input: bool,
}

impl<'a, 'b> ParserIterator<'a, 'b> {
    pub fn next(&mut self) -> Option<Event> {
        let (e, n) = self.parser.next(self.remaining, self.end_of_input);
        self.remaining = &self.remaining[n..];

        if e.is_none() {
            debug_assert!(self.remaining.is_empty());
        }

        e
    }

    pub fn parser(&self) -> &Parser {
        self.parser
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn check_output(gcode: &[u8], expected_events: &[Event]) {
        let mut parser = Parser::new();

        let mut iter_a = parser.iter(gcode, true);
        let mut iter_b = expected_events.iter();

        while let Some(event) = iter_a.next() {
            let expected_event = iter_b.next().unwrap();
            assert_eq!(&event, expected_event);
        }

        assert_eq!(iter_b.next(), None);
    }

    #[test]
    fn parser_works() {
        const TEST_GCODE: &'static [u8] = b"(First Comment)
    
            (Spindle Speed: 0 RPM)
            G21
            G90
            G94 (comment here)

            G01 F40.00 ; Here too
            G00 X8.3800 (And here) Y6.8100
            G00 X1 Y2
            M05
            ";

        let mut parser = Parser::new();

        let expected_events = &[
            Event::Comment(b"First Comment", false),
            Event::EndLine,
            Event::EndLine,
            Event::Comment(b"Spindle Speed: 0 RPM", false),
            Event::EndLine,
            Event::Word(Word {
                key: 'G',
                value: WordValue::RealValue(21.into()),
            }),
            Event::EndLine,
            Event::Word(Word {
                key: 'G',
                value: WordValue::RealValue(90.into()),
            }),
            Event::EndLine,
            Event::Word(Word {
                key: 'G',
                value: WordValue::RealValue(94.into()),
            }),
            Event::Comment(b"comment here", false),
            Event::EndLine,
            Event::EndLine,
            //
            Event::Word(Word {
                key: 'G',
                value: WordValue::RealValue(1.into()),
            }),
            Event::Word(Word {
                key: 'F',
                value: WordValue::RealValue(40.into()),
            }),
            Event::Comment(b" Here too", true),
            Event::EndLine,
            //
            Event::Word(Word {
                key: 'G',
                value: WordValue::RealValue(0.into()),
            }),
            Event::Word(Word {
                key: 'X',
                value: WordValue::RealValue("8.38".parse().unwrap()),
            }),
            Event::Comment(b"And here", false),
            Event::Word(Word {
                key: 'Y',
                value: WordValue::RealValue("6.81".parse().unwrap()),
            }),
            Event::EndLine,
            //
            Event::Word(Word {
                key: 'G',
                value: WordValue::RealValue(0.into()),
            }),
            Event::Word(Word {
                key: 'X',
                value: WordValue::RealValue(1.into()),
            }),
            Event::Word(Word {
                key: 'Y',
                value: WordValue::RealValue(2.into()),
            }),
            Event::EndLine,
            //
            Event::Word(Word {
                key: 'M',
                value: WordValue::RealValue(5.into()),
            }),
            Event::EndLine,
        ];

        check_output(&TEST_GCODE, expected_events);
    }

    #[test]
    fn prusa_start_gcode() {
        let gcode = r#"
            ; printing object 3DBenchy.stl id:0 copy 0
            ; stop printing object 3DBenchy.stl id:0 copy 0

            ;TYPE:Custom
            M862.3 P "MK3S" ; printer model check
            M862.1 P0.4 ; nozzle diameter check
            M115 U3.13.2 ; tell printer latest fw version
            G90 ; use absolute coordinates
            M83 ; extruder relative mode
            M104 S240 ; set extruder temp
            M140 S85 ; set bed temp
            M190 S85 ; wait for bed temp
            M109 S240 ; wait for extruder temp
            G28 W ; home all without mesh bed level
            G80 X95.9944 Y93.6697 W45.5606 H22.6606 ; mesh bed levelling
            "#;

        let expected_events = &[
            Event::EndLine,
            Event::Comment(b" printing object 3DBenchy.stl id:0 copy 0", true),
            Event::EndLine,
            Event::Comment(b" stop printing object 3DBenchy.stl id:0 copy 0", true),
            Event::EndLine,
            Event::EndLine,
            Event::Comment(b"TYPE:Custom", true),
            Event::EndLine,
            //
            Event::Word(Word {
                key: 'M',
                value: WordValue::RealValue("862.3".parse().unwrap()),
            }),
            Event::Word(Word {
                key: 'P',
                value: WordValue::QuotedString("MK3S".into()),
            }),
            Event::Comment(b" printer model check", true),
            Event::EndLine,
            //
            Event::Word(Word {
                key: 'M',
                value: WordValue::RealValue("862.1".parse().unwrap()),
            }),
            Event::Word(Word {
                key: 'P',
                value: WordValue::RealValue("0.4".parse().unwrap()),
            }),
            Event::Comment(b" nozzle diameter check", true),
            Event::EndLine,
            // M115 U3.13.2 ; tell printer latest fw version
            Event::Word(Word {
                key: 'M',
                value: WordValue::RealValue(115.into()),
            }),
            Event::Word(Word {
                key: 'U',
                value: WordValue::UnquotedString("3.13.2".into()),
            }),
            Event::Comment(b" tell printer latest fw version", true),
            Event::EndLine,
            // G90 ; use absolute coordinates
            Event::Word(Word {
                key: 'G',
                value: WordValue::RealValue(90.into()),
            }),
            Event::Comment(b" use absolute coordinates", true),
            Event::EndLine,
            // M83 ; extruder relative mode
            Event::Word(Word {
                key: 'M',
                value: WordValue::RealValue(83.into()),
            }),
            Event::Comment(b" extruder relative mode", true),
            Event::EndLine,
            // M104 S240 ; set extruder temp
            Event::Word(Word {
                key: 'M',
                value: WordValue::RealValue(104.into()),
            }),
            Event::Word(Word {
                key: 'S',
                value: WordValue::RealValue(240.into()),
            }),
            Event::Comment(b" set extruder temp", true),
            Event::EndLine,
            // M140 S85 ; set bed temp
            Event::Word(Word {
                key: 'M',
                value: WordValue::RealValue(140.into()),
            }),
            Event::Word(Word {
                key: 'S',
                value: WordValue::RealValue(85.into()),
            }),
            Event::Comment(b" set bed temp", true),
            Event::EndLine,
            // M190 S85 ; wait for bed temp
            Event::Word(Word {
                key: 'M',
                value: WordValue::RealValue(190.into()),
            }),
            Event::Word(Word {
                key: 'S',
                value: WordValue::RealValue(85.into()),
            }),
            Event::Comment(b" wait for bed temp", true),
            Event::EndLine,
            // M109 S240 ; wait for extruder temp
            Event::Word(Word {
                key: 'M',
                value: WordValue::RealValue(109.into()),
            }),
            Event::Word(Word {
                key: 'S',
                value: WordValue::RealValue(240.into()),
            }),
            Event::Comment(b" wait for extruder temp", true),
            Event::EndLine,
            // G28 W ; home all without mesh bed level
            Event::Word(Word {
                key: 'G',
                value: WordValue::RealValue(28.into()),
            }),
            Event::Word(Word {
                key: 'W',
                value: WordValue::Empty,
            }),
            Event::Comment(b" home all without mesh bed level", true),
            Event::EndLine,
            // G80 X95.9944 Y93.6697 W45.5606 H22.6606 ; mesh bed levelling
            Event::Word(Word {
                key: 'G',
                value: WordValue::RealValue(80.into()),
            }),
            Event::Word(Word {
                key: 'X',
                value: WordValue::RealValue("95.9944".parse().unwrap()),
            }),
            Event::Word(Word {
                key: 'Y',
                value: WordValue::RealValue("93.6697".parse().unwrap()),
            }),
            Event::Word(Word {
                key: 'W',
                value: WordValue::RealValue("45.5606".parse().unwrap()),
            }),
            Event::Word(Word {
                key: 'H',
                value: WordValue::RealValue("22.6606".parse().unwrap()),
            }),
            Event::Comment(b" mesh bed levelling", true),
            Event::EndLine,
        ];

        check_output(gcode.as_bytes(), expected_events);
    }

    #[test]
    fn more_test_cases() {
        let test_cases: &[(&'static str, Vec<Event<'static>>)] = &[
            ("", vec![]),
            ("  \t", vec![]),
            (
                "G1 N4",
                vec![
                    Event::Word(Word {
                        key: 'G',
                        value: WordValue::RealValue(1.into()),
                    }),
                    Event::ParseError,
                    Event::EndLine,
                ],
            ),
            (
                "G00",
                vec![
                    Event::Word(Word {
                        key: 'G',
                        value: WordValue::RealValue(0.into()),
                    }),
                    Event::EndLine,
                ],
            ),
            (
                "G00 X0",
                vec![
                    Event::Word(Word {
                        key: 'G',
                        value: WordValue::RealValue(0.into()),
                    }),
                    Event::Word(Word {
                        key: 'X',
                        value: WordValue::RealValue(0.into()),
                    }),
                    Event::EndLine,
                ],
            ),
            (
                "G29 X Y",
                vec![
                    Event::Word(Word {
                        key: 'G',
                        value: WordValue::RealValue(29.into()),
                    }),
                    Event::Word(Word {
                        key: 'X',
                        value: WordValue::Empty,
                    }),
                    Event::Word(Word {
                        key: 'Y',
                        value: WordValue::Empty,
                    }),
                    Event::EndLine,
                ],
            ),
            (
                "G29 X (hello) Y",
                vec![
                    Event::Word(Word {
                        key: 'G',
                        value: WordValue::RealValue(29.into()),
                    }),
                    Event::Word(Word {
                        key: 'X',
                        value: WordValue::Empty,
                    }),
                    Event::Comment(b"hello", false),
                    Event::Word(Word {
                        key: 'Y',
                        value: WordValue::Empty,
                    }),
                    Event::EndLine,
                ],
            ),
            ("N004", vec![Event::LineNumber(4), Event::EndLine]),
            (
                "N1X2",
                vec![
                    Event::LineNumber(1),
                    Event::Word(Word {
                        key: 'X',
                        value: WordValue::RealValue(2.into()),
                    }),
                    Event::EndLine,
                ],
            ),
            (
                "G1 F8640;_WIPE",
                vec![
                    Event::Word(Word {
                        key: 'G',
                        value: WordValue::RealValue(1.into()),
                    }),
                    Event::Word(Word {
                        key: 'F',
                        value: WordValue::RealValue(8640.into()),
                    }),
                    Event::Comment(b"_WIPE", true),
                    Event::EndLine,
                ],
            ),
            (
                "N1 N2",
                vec![Event::LineNumber(1), Event::ParseError, Event::EndLine],
            ),
            ("N10000", vec![Event::LineNumber(10000), Event::EndLine]),
            ("N99999", vec![Event::LineNumber(99999), Event::EndLine]),
            ("N00010", vec![Event::LineNumber(10), Event::EndLine]),
            ("N123456", vec![Event::ParseError, Event::EndLine]),
            (
                "XY",
                vec![
                    Event::Word(Word {
                        key: 'X',
                        value: WordValue::Empty,
                    }),
                    Event::Word(Word {
                        key: 'Y',
                        value: WordValue::Empty,
                    }),
                    Event::EndLine,
                ],
            ),
            (
                "X1\r",
                vec![
                    Event::Word(Word {
                        key: 'X',
                        value: WordValue::RealValue(1.into()),
                    }),
                    Event::EndLine,
                ],
            ),
            (
                "X1\r\n",
                vec![
                    Event::Word(Word {
                        key: 'X',
                        value: WordValue::RealValue(1.into()),
                    }),
                    Event::EndLine,
                ],
            ),
            (
                "X1\n",
                vec![
                    Event::Word(Word {
                        key: 'X',
                        value: WordValue::RealValue(1.into()),
                    }),
                    Event::EndLine,
                ],
            ),
            ("(hello", vec![Event::ParseError, Event::EndLine]),
            ("(hello\n", vec![Event::ParseError, Event::EndLine]),
            (
                "g0x +0. 12 34y 7",
                vec![
                    Event::Word(Word {
                        key: 'G',
                        value: WordValue::RealValue(0.into()),
                    }),
                    Event::Word(Word {
                        key: 'X',
                        value: WordValue::RealValue("0.1234".parse().unwrap()),
                    }),
                    Event::Word(Word {
                        key: 'Y',
                        value: WordValue::RealValue(7.into()),
                    }),
                    Event::EndLine,
                ],
            ),
            (
                "g0 x+0.1234 y7",
                vec![
                    Event::Word(Word {
                        key: 'G',
                        value: WordValue::RealValue(0.into()),
                    }),
                    Event::Word(Word {
                        key: 'X',
                        value: WordValue::RealValue("0.1234".parse().unwrap()),
                    }),
                    Event::Word(Word {
                        key: 'Y',
                        value: WordValue::RealValue(7.into()),
                    }),
                    Event::EndLine,
                ],
            ),
            (
                "X1..2",
                vec![
                    Event::Word(Word {
                        key: 'X',
                        value: WordValue::RealValue("1.2".parse().unwrap()),
                    }),
                    Event::EndLine,
                ],
            ),
            (
                "X..2",
                vec![
                    Event::Word(Word {
                        key: 'X',
                        value: WordValue::RealValue("0.2".parse().unwrap()),
                    }),
                    Event::EndLine,
                ],
            ),
            (
                "X.2",
                vec![
                    Event::Word(Word {
                        key: 'X',
                        value: WordValue::RealValue("0.2".parse().unwrap()),
                    }),
                    Event::EndLine,
                ],
            ),
        ];

        for (input, expected) in test_cases {
            check_output(input.as_bytes(), &expected);
        }
    }

    // Extra tests to add:
    // <A very long line that exceeds 256 characters>
    // X2.Y
    // X2.

    // TODO: For all test cases, test using incremental inputs (not the whole
    // input available)

    /*
    TODO: Quoted string types to support:
        M587 S"MYROUTER" P"ABCxyz;"" 123"
        M587 S"MYROUTER" P"ABC'X'Y'Z;"" 123"

    */
}
