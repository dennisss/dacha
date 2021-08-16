use std::borrow::Borrow;
use std::str;

use common::bytes::Bytes;
use common::errors::*;

// TODO: Enforce some reasonable limit (for really large sizes approaching this
// limit, we probably want to implement a stream interface rather than storing
// the whole thing in memory)
const BULK_STRING_SIZE_LIMIT: i64 = 512 * 1024 * 1024;

// Constants for all of the type bytes in the protocol
const RESP_SIMPLE_STRING: u8 = '+' as u8;
const RESP_ERROR: u8 = '-' as u8;
const RESP_INTEGER: u8 = ':' as u8;
const RESP_BULK_STRING: u8 = '$' as u8;
const RESP_ARRAY: u8 = '*' as u8;

#[derive(PartialEq, Clone)]
pub enum RESPObject {
    Array(Vec<RESPObject>),
    SimpleString(Bytes),
    Error(Bytes),
    BulkString(Bytes),
    Integer(i64),
    Nil,
}

impl RESPObject {
    /// Serializes the object appending it to the end of the given buffer
    pub fn serialize_to(&self, out: &mut Vec<u8>) {
        let eol = |o: &mut Vec<u8>| o.extend(b"\r\n");

        match self {
            RESPObject::Array(arr) => {
                out.push(RESP_ARRAY);
                out.extend(arr.len().to_string().bytes());
                eol(out);
                for item in arr.iter() {
                    item.serialize_to(out);
                }
            }
            RESPObject::SimpleString(arr) => {
                out.push(RESP_SIMPLE_STRING);
                out.extend_from_slice(&arr);
                eol(out);
            }
            RESPObject::Error(arr) => {
                out.push(RESP_ERROR);
                out.extend(arr.iter());
                eol(out);
            }
            RESPObject::BulkString(arr) => {
                out.push(RESP_BULK_STRING);
                out.extend(arr.len().to_string().bytes());
                eol(out);
                out.extend_from_slice(&arr);
                eol(out);
            }
            RESPObject::Integer(val) => {
                out.push(RESP_INTEGER);
                out.extend(val.to_string().bytes());
                eol(out);
            }
            RESPObject::Nil => {
                out.extend(b"*-1\r\n");
            }
        };
    }

    /// Converts the object into the format of an incoming client request which
    /// should always be an array of bulk strings TODO: Main issue is that
    /// this makes the output less debuggable as it doesn't use the MaybeString
    /// type
    pub fn into_command(self) -> Result<RESPCommand> {
        match self {
            RESPObject::Array(arr) => {
                let mut out = vec![];
                out.reserve_exact(arr.len());

                for item in arr {
                    if let RESPObject::BulkString(data) = item {
                        out.push(data.into());
                    } else {
                        return Err(err_msg("Some items are not bulk strings"));
                    }
                }

                Ok(out)
            }
            _ => Err(err_msg("Not an array")),
        }
    }

    pub fn contains_nil(&self) -> bool {
        match self {
            RESPObject::Nil => true,
            RESPObject::Array(arr) => {
                for item in arr.iter() {
                    if item.contains_nil() {
                        return true;
                    }
                }

                false
            }
            _ => false,
        }
    }
}

#[derive(Clone)]
pub struct RESPString(Bytes);

impl From<Vec<u8>> for RESPString {
    fn from(arr: Vec<u8>) -> Self {
        RESPString(arr.into())
    }
}
impl From<Bytes> for RESPString {
    fn from(arr: Bytes) -> Self {
        RESPString(arr)
    }
}
impl Into<Bytes> for RESPString {
    fn into(self) -> Bytes {
        self.0
    }
}
impl Borrow<[u8]> for RESPString {
    fn borrow(&self) -> &[u8] {
        &self.0
    }
}
impl AsRef<[u8]> for RESPString {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}
impl std::ops::Deref for RESPString {
    type Target = [u8];
    fn deref(&self) -> &[u8] {
        &self.0
    }
}

impl std::fmt::Debug for RESPString {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        MaybeString(&self.0).fmt(f)
    }
}

pub type RESPCommand = Vec<RESPString>;

// A wrapper around an array of bytes which might look like to be formatted as a
// utf8 string
struct MaybeString<'a>(&'a [u8]);
impl<'a> std::fmt::Debug for MaybeString<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let arr = &self.0;
        if arr.len() < 128 {
            if let Ok(s) = str::from_utf8(&arr) {
                return write!(f, "{}", s);
            }
        }

        write!(f, "{:?}", arr)
    }
}

impl std::fmt::Debug for RESPObject {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            RESPObject::Array(arr) => write!(f, "Array({:?})", arr),
            RESPObject::SimpleString(arr) => write!(f, "Simple({:?})", MaybeString(&arr)),
            RESPObject::Error(arr) => write!(f, "Error({:?})", MaybeString(&arr)),
            RESPObject::BulkString(arr) => write!(f, "Bulk({:?})", MaybeString(&arr)),
            RESPObject::Integer(val) => write!(f, "Int({})", val),
            RESPObject::Nil => write!(f, "Nil"),
        }
    }
}

// All the types of objects that use a number as the first line
enum LengthKind {
    Integer,
    BulkString,
    Array,
}

// These are all represented as a single line-terminated sequence of characters
enum StringKind {
    SimpleString,
    Error,
}

/// The current state of the parser
enum RESPState {
    // Start state: next byte to be read is the object type
    Type,

    // Used for reading line terminated numbers
    LengthSign(LengthKind),
    Length {
        kind: LengthKind,
        accum: i64,
        sign: i8,
    },
    LengthEnd {
        kind: LengthKind,
        accum: i64,
        sign: i8,
    }, // pending a '\n'

    /// Used for line terminated strings (SimpleString and Error)
    String {
        kind: StringKind,
        data: Vec<u8>,
    },

    /// In this case we are reading a known amount of data for a BulkString
    Data {
        len: usize,
        data: Vec<u8>,
    },

    Inline {
        arr: Vec<RESPObject>,
        cur: Option<Vec<u8>>,
    },

    End1(RESPObject), // pending '\r\n'
    End2(RESPObject), // pending '\n'
}

#[derive(PartialEq)]
enum InputMode {
    Full,
    Inline,
}

// will be used to track arrays that we are currently building
struct RESPStackEntry {
    // Current contents of the array
    arr: Vec<RESPObject>,

    // Expected length of the array
    len: usize,
}

/// Stateful parser for the RESP protocol described here: https://redis.io/topics/protocol
/// Supports both the regular format and the inline command format, but will
/// reject intermixing of the two formats
pub struct RESPParser {
    state: Option<(RESPState, Vec<RESPStackEntry>)>,
    mode: Option<InputMode>,
}

/// Represents a single transition produced while processing zero or more bytes
enum Step {
    NextState(RESPState),
    Produce(RESPObject),
}
use self::Step::*;

impl RESPParser {
    pub fn new() -> Self {
        RESPParser {
            state: None,
            mode: None,
        }
    }

    /// Given some number of bytes, This will incrementally parse objects from
    /// it returning how many bytes were consumed and whether or not an object
    /// was produced If all bytes in the input were not produced, then an
    /// object was produced and this function should be called sequentially with
    /// the rest of the input
    pub fn parse(&mut self, buf: &[u8]) -> Result<(usize, Option<RESPObject>)> {
        let (mut state, mut stack) = match self.state.take() {
            Some(v) => v,
            None => (RESPState::Type, vec![]),
        };

        // Tracks the number of bytes consumed from the input
        let mut i = 0;

        while i < buf.len() {
            let c = buf[i];

            let out: Step = match state {
                RESPState::Type => {
                    i += 1;

                    let mut new_mode = InputMode::Full;

                    let ret = NextState(match c {
                        RESP_SIMPLE_STRING => RESPState::String {
                            kind: StringKind::SimpleString,
                            data: vec![],
                        },
                        RESP_ERROR => RESPState::String {
                            kind: StringKind::Error,
                            data: vec![],
                        },
                        RESP_INTEGER => RESPState::LengthSign(LengthKind::Integer),
                        RESP_BULK_STRING => RESPState::LengthSign(LengthKind::BulkString),
                        RESP_ARRAY => RESPState::LengthSign(LengthKind::Array),
                        // Otherwise we are in inline-command mode (space separated bulk strings
                        // ending in a reference) (although the redis
                        // documentation states that this could be for anything other than '*' for
                        // command mode)
                        _ => {
                            new_mode = InputMode::Inline;
                            RESPState::Inline {
                                arr: vec![],
                                cur: Some(vec![c]),
                            }
                            //println!("{}:: {:?}", i, buf);
                            //return Err(err_msg("Invalid type byte"))
                        }
                    });

                    // For a single parser, we latch onto a single RESP mode and enforce that all
                    // objects in the future are parsed using this mode. A client should only ever
                    // choose to use one mode or another and we don't want to allow an inline
                    // command to appear inside of a full-style array
                    if let Some(ref m) = self.mode {
                        if *m != new_mode {
                            return Err(err_msg(
                                "Started new object in different mode than last time",
                            ));
                        }
                    } else {
                        self.mode = Some(new_mode);
                    }

                    ret
                }
                RESPState::Inline { mut arr, mut cur } => {
                    i += 1;

                    if c == (' ' as u8) {
                        if let Some(data) = cur.take() {
                            arr.push(RESPObject::BulkString(data.into()));
                        }

                        NextState(RESPState::Inline { arr, cur })
                    } else if c == ('\r' as u8) {
                        if let Some(data) = cur.take() {
                            arr.push(RESPObject::BulkString(data.into()));
                        }

                        NextState(RESPState::End2(RESPObject::Array(arr)))
                    } else {
                        let mut data = match cur {
                            Some(v) => v,
                            None => vec![],
                        };

                        data.push(c);
                        cur = Some(data);

                        NextState(RESPState::Inline { arr, cur })
                    }
                }
                RESPState::LengthSign(kind) => {
                    NextState(if c == ('-' as u8) {
                        i += 1;
                        RESPState::Length {
                            kind,
                            accum: 0,
                            sign: -1,
                        }
                    } else {
                        // Will reparse this character as a digit
                        RESPState::Length {
                            kind,
                            accum: 0,
                            sign: 1,
                        }
                    })
                }
                RESPState::Length { kind, accum, sign } => {
                    i += 1;

                    NextState(if c == ('\r' as u8) {
                        RESPState::LengthEnd { kind, accum, sign }
                    } else {
                        let zero = '0' as u8;
                        let nine = '9' as u8;

                        let digit = if c >= zero && c <= nine {
                            c - zero
                        } else {
                            return Err(err_msg("Invalid digit"));
                        };

                        let acc = (accum * 10) + (digit as i64);

                        RESPState::Length {
                            kind,
                            sign,
                            accum: acc,
                        }
                    })
                }
                RESPState::LengthEnd { kind, accum, sign } => {
                    i += 1;

                    if c != ('\n' as u8) {
                        return Err(err_msg("Missing new line after number"));
                    }

                    let val = accum * (sign as i64);

                    match kind {
                        LengthKind::Integer => Produce(RESPObject::Integer(val)),
                        LengthKind::Array => {
                            if val < 0 {
                                Produce(RESPObject::Nil)
                            } else if val == 0 {
                                Produce(RESPObject::Array(vec![]))
                            } else {
                                stack.push(RESPStackEntry {
                                    arr: vec![],
                                    len: val as usize,
                                });

                                // Next time should parse the first item of the array
                                NextState(RESPState::Type)
                            }
                        }
                        LengthKind::BulkString => {
                            if val < 0 {
                                Produce(RESPObject::Nil)
                            } else if val == 0 {
                                NextState(RESPState::End1(RESPObject::BulkString(Bytes::new())))
                            } else {
                                if val > BULK_STRING_SIZE_LIMIT {
                                    return Err(err_msg("Bulk string is too large"));
                                }

                                let len = val as usize;
                                let mut data = vec![];
                                data.reserve_exact(len);

                                NextState(RESPState::Data { len, data })
                            }
                        }
                    }
                }

                RESPState::String { kind, mut data } => {
                    i += 1;

                    if c == ('\r' as u8) {
                        NextState(RESPState::End2(match kind {
                            StringKind::Error => RESPObject::Error(data.into()),
                            StringKind::SimpleString => RESPObject::SimpleString(data.into()),
                        }))
                    } else if c == ('\n' as u8) {
                        return Err(err_msg("Received NL before CR"));
                    } else {
                        // Save the character and transition back to the same state
                        data.push(c);
                        NextState(RESPState::String { kind, data })
                    }
                }

                RESPState::Data { len, mut data } => {
                    // Here we will try to take bytes for it if possible

                    // TODO: In the case of us getting a Bytes object, we may be able to zero-copy
                    // the entire data segment out of the input without and explicit extend

                    let take = std::cmp::min(len - data.len(), buf.len() - i);
                    data.extend_from_slice(&buf[i..(i + take)]);
                    i += take;

                    if data.len() == len {
                        NextState(RESPState::End1(RESPObject::BulkString(data.into())))
                    } else {
                        NextState(RESPState::Data { len, data })
                    }
                }
                RESPState::End1(obj) => {
                    i += 1;

                    if c != ('\r' as u8) {
                        return Err(err_msg("No CR after object"));
                    }

                    NextState(RESPState::End2(obj))
                }
                RESPState::End2(obj) => {
                    i += 1;

                    if c != ('\n' as u8) {
                        return Err(err_msg("No NL after Object"));
                    }

                    Produce(obj)
                }
            };

            match out {
                NextState(s) => {
                    state = s;
                }
                // If we produced a complete object, then we must check if we are inside of an array
                Produce(mut obj) => {
                    loop {
                        // If we are in an array, we will add the produced object to that array
                        if let Some(mut ent) = stack.pop() {
                            // Add the new item to the array
                            ent.arr.push(obj);

                            // If the array is complete, bubble up with that array as the new
                            // production
                            if ent.arr.len() == ent.len {
                                obj = RESPObject::Array(ent.arr)
                            }
                            // Otherwise there are more items left in the array to be parsed
                            else {
                                state = RESPState::Type;
                                stack.push(ent);
                                break;
                            }
                        }
                        // Otherwise we are the top-level object and we can return it
                        else {
                            return Ok((i, Some(obj)));
                        }
                    }
                }
            };
        }

        // If we got here then we didn't produce any object
        self.state = Some((state, stack));
        Ok((i, None))
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn parsing_serialize_simple() {
        // Most of these test cases are from the protocol description page
        let cases: Vec<(&'static [u8], RESPObject)> = vec![
            (b"+OK\r\n", RESPObject::SimpleString(b"OK"[..].into())),
            (
                b"-Error message\r\n",
                RESPObject::Error(b"Error message"[..].into()),
            ),
            (b":1000\r\n", RESPObject::Integer(1000)),
            (b":0\r\n", RESPObject::Integer(0)),
            (b":-12323\r\n", RESPObject::Integer(-12323)),
            (
                b"$6\r\nfoobar\r\n",
                RESPObject::BulkString(b"foobar"[..].into()),
            ),
            (b"$0\r\n\r\n", RESPObject::BulkString(Bytes::new())),
            (b"$-1\r\n", RESPObject::Nil),
            (b"*0\r\n", RESPObject::Array(vec![])),
            (
                b"*2\r\n$3\r\nfoo\r\n$3\r\nbar\r\n",
                RESPObject::Array(vec![
                    RESPObject::BulkString(b"foo"[..].into()),
                    RESPObject::BulkString(b"bar"[..].into()),
                ]),
            ),
            (
                b"*3\r\n:1\r\n:2\r\n:3\r\n",
                RESPObject::Array(vec![
                    RESPObject::Integer(1),
                    RESPObject::Integer(2),
                    RESPObject::Integer(3),
                ]),
            ),
            (
                b"*5\r\n:1\r\n:2\r\n:3\r\n:4\r\n$6\r\nfoobar\r\n",
                RESPObject::Array(vec![
                    RESPObject::Integer(1),
                    RESPObject::Integer(2),
                    RESPObject::Integer(3),
                    RESPObject::Integer(4),
                    RESPObject::BulkString(b"foobar"[..].into()),
                ]),
            ),
            (b"*-1\r\n", RESPObject::Nil),
            (
                b"*2\r\n*3\r\n:1\r\n:2\r\n:3\r\n*2\r\n+Foo\r\n-Bar\r\n",
                RESPObject::Array(vec![
                    RESPObject::Array(vec![
                        RESPObject::Integer(1),
                        RESPObject::Integer(2),
                        RESPObject::Integer(3),
                    ]),
                    RESPObject::Array(vec![
                        RESPObject::SimpleString(b"Foo"[..].into()),
                        RESPObject::Error(b"Bar"[..].into()),
                    ]),
                ]),
            ),
            (
                b"*3\r\n$3\r\nfoo\r\n$-1\r\n$3\r\nbar\r\n",
                RESPObject::Array(vec![
                    RESPObject::BulkString(b"foo"[..].into()),
                    RESPObject::Nil,
                    RESPObject::BulkString(b"bar"[..].into()),
                ]),
            ),
            (
                b"*1\r\n*0\r\n",
                RESPObject::Array(vec![RESPObject::Array(vec![])]),
            ),
        ];

        for (s, obj) in cases.iter() {
            let mut parser = RESPParser::new();
            assert_eq!(parser.parse(s), Ok((s.len(), Some(obj.clone()))));

            // Trying the reverse direction
            // In the general case we can't check the serialization containing nils as it
            // may have two forms
            if obj.contains_nil() {
                continue;
            }

            let mut ser = vec![];
            obj.serialize_to(&mut ser);
            assert_eq!(&ser, s);
        }

        // Other cases to fuzz
        // One concatenation of valid packets is valid (under one parser)
        // For that concatenation,
    }

    #[test]
    fn parsing_inline() {
        let cases: Vec<(&'static [u8], RESPObject)> = vec![(
            b"PING\r\n",
            RESPObject::Array(vec![RESPObject::BulkString(b"PING"[..].into())]),
        )];

        for (s, obj) in cases.iter() {
            let mut parser = RESPParser::new();
            assert_eq!(parser.parse(s), Ok((s.len(), Some(obj.clone()))));
        }
    }

    #[test]
    fn nil_serialize() {
        // This is a separate test than the simple one as it assumes that we use
        // a specific serialization case consistently for
    }

    #[test]
    fn failure_cases() {

        // Fail on one bad packet

        // Fail on good packet followed by bad packet

        //
    }
}
