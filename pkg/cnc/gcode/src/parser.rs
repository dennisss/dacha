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

#[derive(Debug, PartialEq)]
pub enum Event {
    Word(Word),
    EndLine,
    ParseError,
}

#[derive(Default)]
pub struct Parser {
    in_error_line: bool,
    in_semi_colon_comment: bool,
    in_paren_comment: bool,
}

impl Parser {
    /// NOTE: Only complete lines can be passed
    pub fn parse<'a>(&mut self, mut data: &'a [u8]) -> (Option<Event>, &'a [u8]) {
        let mut event = None;
        while !data.is_empty() {
            // Skip whitespace
            if let Some(m) = WHITESPACE.exec(data) {
                data = &data[m.last_index()..];
                continue;
            }

            let c = data[0];
            if c == b'\n' || c == b'\r' {
                if self.in_paren_comment && !self.in_error_line {
                    self.in_error_line = true;
                    event = Some(Event::ParseError);
                    break;
                }

                data = &data[1..];

                // Reset everything at the end of a line.
                self.in_paren_comment = false;
                self.in_semi_colon_comment = false;
                self.in_error_line = false;

                event = Some(Event::EndLine);
                break;
            }

            // When there is an error in a line, just skip ahead until the line is done.
            if self.in_error_line {
                data = &data[1..];
                continue;
            }

            if self.in_semi_colon_comment {
                data = &data[1..];
                continue;
            }

            if self.in_paren_comment {
                data = &data[1..];
                if c == b')' {
                    self.in_paren_comment = false;
                }

                continue;
            }

            if c == b'(' {
                data = &data[1..];
                self.in_paren_comment = true;
                continue;
            }

            if c == b';' {
                data = &data[1..];
                self.in_semi_colon_comment = true;
                continue;
            }

            let (word, rest) = match Self::parse_word(data) {
                Some(v) => v,
                None => {
                    self.in_error_line = true;
                    event = Some(Event::ParseError);
                    break;
                }
            };

            data = rest;
            event = Some(Event::Word(word));
            break;
        }

        (event, data)
    }

    fn parse_word(mut data: &[u8]) -> Option<(Word, &[u8])> {
        let m = match WORD.exec(data) {
            Some(v) => v,
            None => return None,
        };

        // Skip everything but the last whitespace which isn't part of the word and may
        // need to be parsed as a new line.
        data = &data[m.group(1).unwrap().len()..];

        let key = m.group(2).unwrap()[0];

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

        let mut parser = Parser::default();
        let mut rest = TEST_GCODE;
        while !rest.is_empty() {
            let (e, r) = parser.parse(rest);
            if let Some(e) = e {
                events.push(e);
            } else {
                assert!(r.is_empty());
            }

            rest = r;
        }

        assert_eq!(
            &events,
            &[
                Event::EndLine,
                Event::EndLine,
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
