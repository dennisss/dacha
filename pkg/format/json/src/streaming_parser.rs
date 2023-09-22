use std::cell::{Cell, RefCell};

use common::errors::*;

#[derive(Debug, Clone)]
pub enum ParsingEvent {
    ObjectStart,
    ObjectEnd,
    ArrayStart,
    ArrayEnd,
    String(String),
    Number(f64),
    Bool(bool),
    Null,
}

pub struct StreamingParser<'a> {
    state: ParsingState,

    remaining: &'a str,

    /// Once we are done the current array/object, we will pop from this to tell
    /// what our next state should be.
    ///
    /// TODO: Limit max depth.
    stack: Vec<ParsingState>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ParsingState {
    /// Waiting to parse the root value. Nothing has been parsed yet.
    Start,

    /// The root value has been parsed. No more data is expected.
    End,

    /// Immediately after a '{'.
    ObjectStart,

    /// Immediately after a key in an object has been parsed (next token
    /// expected is a ':').
    ObjectValueStart,

    /// Immediately after a value in an object has been parsed (next token is a
    /// ',' or '}').
    ObjectValueEnd,

    /// Immediately after a '['.
    ArrayStart,

    /// Immediately after a value in an array has been parsed (next token is a
    /// ',' or ']').
    ArrayValueEnd,
}

impl<'a> StreamingParser<'a> {
    pub fn new(input: &'a str) -> Self {
        Self {
            state: ParsingState::Start,
            remaining: input,
            stack: vec![],
        }
    }

    /// Gets the next event observed in the input.
    ///
    /// - Invalid JSON will trigger an error to be returned (not inconsistent
    ///   events).
    /// - ObjectStart events will always be followed by events of the form:
    ///   `(String ValueEvents+)* ObjectEnd`
    /// - ArrayStart events will always be followed by enents of the form:
    ///   `(ValueEvents+)* ArrayEnd`
    ///
    /// Continue calling until you get None to fully parse the input. An error
    /// will be returned if we have parsed the root object but there is still
    /// remaining non-whitespace input.
    pub fn next(&mut self) -> Result<Option<ParsingEvent>> {
        self.skip_whitespace();
        match self.state {
            ParsingState::Start => {
                self.stack.push(ParsingState::End);
                let v = self.enter_value()?;
                Ok(Some(v))
            }
            ParsingState::End => Ok(None),
            ParsingState::ObjectStart => {
                if !self.remaining.is_empty() && self.remaining.as_bytes()[0] == b'}' {
                    self.remaining = &self.remaining[1..];
                    self.pop_stack()?;
                    return Ok(Some(ParsingEvent::ObjectEnd));
                }

                self.state = ParsingState::ObjectValueStart;
                let key = self.parse_string()?;
                Ok(Some(ParsingEvent::String(key)))
            }
            ParsingState::ObjectValueStart => {
                if self.remaining.is_empty() || self.remaining.as_bytes()[0] != b':' {
                    return Err(err_msg("Missing colon after key"));
                }
                self.remaining = &self.remaining[1..];
                self.skip_whitespace();

                self.stack.push(ParsingState::ObjectValueEnd);
                Ok(Some(self.enter_value()?))
            }
            ParsingState::ObjectValueEnd => {
                if self.remaining.is_empty() {
                    return Err(err_msg("Expected more tokens after object value"));
                }

                let c = self.remaining.as_bytes()[0];
                if c == b'}' {
                    self.remaining = &self.remaining[1..];
                    self.pop_stack()?;
                    return Ok(Some(ParsingEvent::ObjectEnd));
                }

                if c != b',' {
                    return Err(err_msg("Expected comma or end bracker after object value"));
                }
                self.remaining = &self.remaining[1..];
                self.skip_whitespace();

                // Parse another key.
                self.state = ParsingState::ObjectValueStart;
                let key = self.parse_string()?;
                Ok(Some(ParsingEvent::String(key)))
            }
            ParsingState::ArrayStart => {
                if !self.remaining.is_empty() && self.remaining.as_bytes()[0] == b']' {
                    self.remaining = &self.remaining[1..];
                    self.pop_stack()?;
                    return Ok(Some(ParsingEvent::ArrayEnd));
                }

                self.stack.push(ParsingState::ArrayValueEnd);
                Ok(Some(self.enter_value()?))
            }
            ParsingState::ArrayValueEnd => {
                if self.remaining.is_empty() {
                    return Err(err_msg("Expected more tokens after array value"));
                }

                let c = self.remaining.as_bytes()[0];
                if c == b']' {
                    self.remaining = &self.remaining[1..];
                    self.pop_stack()?;
                    return Ok(Some(ParsingEvent::ArrayEnd));
                }

                if c != b',' {
                    return Err(err_msg("Expected comma or end bracker after object value"));
                }
                self.remaining = &self.remaining[1..];
                self.skip_whitespace();

                self.stack.push(ParsingState::ArrayValueEnd);
                Ok(Some(self.enter_value()?))
            }
        }
    }

    /// NOTE: This must be called after we are done parsing the current state.
    fn pop_stack(&mut self) -> Result<()> {
        self.state = self.stack.pop().unwrap();

        // In case a parser doesn't check for None to be returned from Next, we will
        // verify all input is consumed here.
        if self.state == ParsingState::End {
            self.skip_whitespace();
            if !self.remaining.is_empty() {
                return Err(err_msg("Expected no tokens after end of root value"));
            }
        }

        Ok(())
    }

    /// Parses a quoted string from self.remaining (including the pair of
    /// quotes).
    fn parse_string(&mut self) -> Result<String> {
        // TODO: If the user is ok with us mutating the input, we can use the input as
        // the buffer for un-escaping the value instead of making a buffer.

        let mut s = String::new();

        if self.remaining.is_empty() || self.remaining.as_bytes()[0] != b'"' {
            return Err(err_msg("Expected double quote to start a string."));
        }
        self.remaining = &self.remaining[1..];

        loop {
            let (v, rest) = crate::parser::parse_character(self.remaining, '"')?;
            self.remaining = rest;

            if let Some(v) = v {
                s.push(v);
            } else {
                break;
            }
        }

        Ok(s)
    }

    /// NOTE: This function should always change to a new state.
    fn enter_value(&mut self) -> Result<ParsingEvent> {
        if self.remaining.is_empty() {
            return Err(err_msg("Expected more tokens to form value"));
        }

        let c = self.remaining.as_bytes()[0];
        match c {
            b'[' => {
                self.remaining = &self.remaining[1..];
                self.state = ParsingState::ArrayStart;
                Ok(ParsingEvent::ArrayStart)
            }
            b'{' => {
                self.remaining = &self.remaining[1..];
                self.state = ParsingState::ObjectStart;
                Ok(ParsingEvent::ObjectStart)
            }
            b'"' => {
                let value = self.parse_string()?;
                self.pop_stack()?;
                Ok(ParsingEvent::String(value))
            }
            b't' => {
                self.remaining = self
                    .remaining
                    .strip_prefix("true")
                    .ok_or_else(|| err_msg("Not true"))?;
                self.pop_stack()?;
                Ok(ParsingEvent::Bool(true))
            }
            b'f' => {
                self.remaining = self
                    .remaining
                    .strip_prefix("false")
                    .ok_or_else(|| err_msg("Not false"))?;
                self.pop_stack()?;
                Ok(ParsingEvent::Bool(false))
            }
            b'n' => {
                self.remaining = self
                    .remaining
                    .strip_prefix("null")
                    .ok_or_else(|| err_msg("Not null"))?;
                self.pop_stack()?;
                Ok(ParsingEvent::Null)
            }
            _ => {
                let (num, rest) = crate::parser::parse_number(self.remaining)?;
                self.remaining = rest;
                self.pop_stack()?;
                Ok(ParsingEvent::Number(num))
            }
        }
    }

    fn skip_whitespace(&mut self) {
        // NOTE: A unicode character will never start with an ASCII character so this
        // will always keep the string located at a valid code point.
        while !self.remaining.is_empty() && self.remaining.as_bytes()[0].is_ascii_whitespace() {
            self.remaining = &self.remaining[1..];
        }
    }
}
