use base_error::*;

use crate::decimal::Decimal;

regexp!(WHITESPACE => "^[ \t]+");

// TODO: Deduplicate the decimal pattern with the decimal.rs file.
//
regexp!(WORD => "^(([A-Z])((?:[+-]?)(?:[0-9]*)\\.?(?:[0-9]*)))(?:[ \t\n\r]|$)");

#[derive(Debug, PartialEq)]
pub struct Word {
    // TODO: Use an 'AScii char' type for this.
    pub key: char,

    pub value: Decimal,
}

pub enum WordValue {
    RealValue(Decimal),
    QuotedString(String),
    UnquotedString(String),
    Empty,
}

#[derive(Debug, PartialEq)]
pub enum Event<'a> {
    Word(Word),
    EndLine,
    ParseError,
    Comment(&'a [u8]),
}

pub struct Parser<'a> {
    // state: ParserState,
    data: &'a [u8],

    remaining: &'a [u8],

    line_number: usize,

    /// If true, the current line we are parsing has an error.
    /// This will be reset once we hit a
    in_error_line: bool,

    in_semi_colon_comment: bool,
    in_paren_comment: bool,
    comment_start_offset: usize,
}

impl<'a> Parser<'a> {
    /// NOTE: Only complete lines can be passed
    pub fn new(data: &'a [u8]) -> Self {
        Self {
            // state: ParserState::StartOfLine,
            data,
            remaining: data,
            line_number: 1,
            in_error_line: false,
            in_semi_colon_comment: false,
            in_paren_comment: false,
            comment_start_offset: 0,
        }
    }

    pub fn next(&mut self) -> Option<Event<'a>> {
        let mut event = None;

        // let mut last_size = 0;
        while !self.remaining.is_empty() {
            // assert!(last_size != data.len());
            // last_size = data.len();

            // match

            // Skip whitespace
            if let Some(m) = WHITESPACE.exec(self.remaining) {
                self.remaining = &self.remaining[m.last_index()..];
                continue;
            }

            let c = self.remaining[0];

            // TODO: Should merge consecutive line endings.
            if c == b'\n' || c == b'\r' {
                if self.in_paren_comment && !self.in_error_line {
                    self.in_error_line = true;
                    event = Some(Event::ParseError);
                    break;
                }

                if self.in_semi_colon_comment {
                    self.in_semi_colon_comment = false;
                    event = Some(Event::Comment(
                        &self.data[self.comment_start_offset..self.offset()],
                    ));
                    break;
                }

                self.remaining = &self.remaining[1..];

                // Reset everything at the end of a line.
                self.in_paren_comment = false;
                self.in_semi_colon_comment = false;
                self.in_error_line = false;
                self.line_number += 1;

                event = Some(Event::EndLine);
                break;
            }

            // When there is an error in a line, just skip ahead until the line is done.
            if self.in_error_line {
                self.remaining = &self.remaining[1..];
                continue;
            }

            if self.in_semi_colon_comment {
                self.remaining = &self.remaining[1..];

                continue;
            }

            if self.in_paren_comment {
                self.remaining = &self.remaining[1..];

                if c == b')' {
                    self.in_paren_comment = false;
                    event = Some(Event::Comment(
                        &self.data[self.comment_start_offset..self.offset() - 1],
                    ));
                    break;
                }

                continue;
            }

            if c == b'(' {
                self.remaining = &self.remaining[1..];

                self.in_paren_comment = true;
                self.comment_start_offset = self.offset();
                continue;
            }

            if c == b';' {
                self.remaining = &self.remaining[1..];

                self.in_semi_colon_comment = true;
                self.comment_start_offset = self.offset();
                continue;
            }

            let (word, rest) = match Self::parse_word(self.remaining) {
                Some(v) => v,
                None => {
                    self.in_error_line = true;
                    // println!(
                    //     "Word parse fail: {:?}",
                    //     std::str::from_utf8(self.remaining).unwrap()
                    // );
                    event = Some(Event::ParseError);
                    break;
                }
            };

            self.remaining = rest;
            event = Some(Event::Word(word));
            break;
        }

        event
    }

    /// Current line number (incremented after an EndLine event is emitted)
    pub fn current_line_number(&self) -> usize {
        self.line_number
    }

    pub fn offset(&self) -> usize {
        self.data.len() - self.remaining.len()
    }

    fn parse_word(mut data: &[u8]) -> Option<(Word, &[u8])> {
        // TODO: Ideally make this partial.

        let m = match WORD.exec(data) {
            Some(v) => v,
            None => return None,
        };

        // Skip everything but the last whitespace which isn't part of the word and may
        // need to be parsed as a new line.
        data = &data[m.group(1).unwrap().len()..];

        let key = m.group(2).unwrap()[0];

        // TODO: Incorporate this into the regular expression.
        if key == b'N' {
            return None;
        }

        // TODO: Deduplicate this stuff with Decimal::parse()

        let (decimal, rest) = match Decimal::parse(m.group(3).unwrap()) {
            Some(v) => v,
            None => return None,
        };

        if !rest.is_empty() {
            return None;
        }

        Some((
            Word {
                key: key as char,
                value: decimal,
            },
            data,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn parser_works() {
        let mut events = vec![];

        let mut parser = Parser::new(TEST_GCODE);
        while let Some(e) = parser.next() {
                events.push(e);
            } else {
                assert!(r.is_empty());
            }

            rest = r;
        }

        assert_eq!(
            &events,
            &[
                Event::Comment(b"First Comment"),
                Event::EndLine,
                Event::EndLine,
                Event::Comment(b"Spindle Speed: 0 RPM"),
                Event::EndLine,
                Event::Word(Word {
                    key: 'G',
                    value: 21.into()
                }),
                Event::EndLine,
                Event::Word(Word {
                    key: 'G',
                    value: 90.into()
                }),
                Event::EndLine,
                Event::Word(Word {
                    key: 'G',
                    value: 94.into()
                }),
                Event::Comment(b"comment here"),
                Event::EndLine,
                Event::EndLine,
                //
                Event::Word(Word {
                    key: 'G',
                    value: 1.into()
                }),
                Event::Word(Word {
                    key: 'F',
                    value: 40.into()
                }),
                Event::Comment(b" Here too"),
                Event::EndLine,
                //
                Event::Word(Word {
                    key: 'G',
                    value: 0.into()
                }),
                Event::Word(Word {
                    key: 'X',
                    value: "8.38".parse().unwrap()
                }),
                Event::Comment(b"And here"),
                Event::Word(Word {
                    key: 'Y',
                    value: "6.81".parse().unwrap()
                }),
                Event::EndLine,
                //
                Event::Word(Word {
                    key: 'G',
                    value: 0.into()
                }),
                Event::Word(Word {
                    key: 'X',
                    value: 1.into()
                }),
                Event::Word(Word {
                    key: 'Y',
                    value: 2.into()
                }),
                Event::EndLine,
                //
                Event::Word(Word {
                    key: 'M',
                    value: 5.into()
                }),
                Event::EndLine,
            ]
        );
    }
}
