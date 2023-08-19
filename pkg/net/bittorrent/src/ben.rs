use std::collections::HashMap;
use std::convert::{TryFrom, TryInto};

use automata::regexp::vm::instance::{RegExpMatch, StaticRegExpMatch};
use common::bytes::BytesMut;
use common::errors::*;
use parsing::*;

// String | Integer | List | Dict
regexp!(TAG => "^(?:([1-9][0-9]*|0):|i(-?[1-9][0-9]*|0)e|(l)|(d))");

#[derive(Debug, Clone, PartialEq)]
pub enum BENValue {
    String(BytesMut),
    Integer(isize),
    List(Vec<BENValue>),

    // NOTE: When serialized, it should be in sorted order.
    // TODO: Keys may be non-ascii
    Dict(HashMap<BytesMut, BENValue>),
}

// TODO: For the purpose of hashing, we may want to retain the original input
// mapping of each key.

impl BENValue {
    // enum_accessor!(string, String, String);
    enum_accessor!(int, Integer, isize);
    // enum_accessor!(list, List, Vec<BENValue>);

    pub fn dict(self) -> Result<HashMap<BytesMut, BENValue>> {
        match self {
            Self::Dict(v) => Ok(v),
            _ => Err(err_msg("Not a dict type value")),
        }
    }

    pub fn parse(mut input: &[u8]) -> Result<(Self, &[u8])> {
        let m: StaticRegExpMatch = TAG.exec(input).ok_or_else(|| err_msg("Unknown BEN tag"))?;
        input = &input[m.last_index()..];

        Ok(if let Some(len_str) = m.group_str(1) {
            let len = len_str?.parse::<usize>()?;
            let data = parse_next!(input, take_exact(len)).into();
            (Self::String(data), input)
        } else if let Some(int_str) = m.group_str(2) {
            let int = int_str?.parse::<isize>()?;
            (Self::Integer(int), input)
        } else if m.group_str(3).is_some() {
            // List
            let items = parse_next!(input, many(Self::parse));
            parse_next!(input, tag("e"));
            (Self::List(items), input)
        } else if m.group_str(4).is_some() {
            // Dict
            let items = parse_next!(input, many(Self::parse));
            parse_next!(input, tag("e"));

            if items.len() % 2 != 0 {
                return Err(err_msg("Expected pairs of keys/values"));
            }

            let mut map = HashMap::new();
            for i in 0..(items.len() / 2) {
                let key = match &items[2 * i] {
                    Self::String(s) => s.clone(),
                    v @ _ => {
                        return Err(format_err!("Key must be a string: {:?}", v));
                    }
                };

                // TODO: Verify sorted

                let value = items[2 * i + 1].clone();

                map.insert(key, value);
            }

            // Basically the same procedure as for the list
            (Self::Dict(map), input)
        } else {
            return Err(err_msg("Failed to parse"));
        })
    }

    pub fn serialize(&self, out: &mut Vec<u8>) {
        match self {
            BENValue::String(v) => {
                out.extend_from_slice(format!("{}:", v.len()).as_ref());
                out.extend_from_slice(v.as_ref());
            }
            BENValue::Integer(v) => {
                out.extend_from_slice(format!("i{}e", *v).as_bytes());
            }
            BENValue::List(v) => {
                out.push(b'l');
                for item in v {
                    item.serialize(out);
                }
                out.push(b'e');
            }
            BENValue::Dict(v) => {
                out.push(b'd');

                let mut keys = v.keys().map(|v| v.as_ref()).collect::<Vec<_>>();
                keys.sort();

                for key in keys {
                    // TODO: Deduplicate with the ::String logic.
                    out.extend_from_slice(format!("{}:", key.len()).as_ref());
                    out.extend_from_slice(key.as_ref());

                    let value = v.get(key).unwrap();
                    value.serialize(out);
                }

                out.push(b'e');
            }
        }
    }
}

impl std::convert::From<isize> for BENValue {
    fn from(value: isize) -> BENValue {
        BENValue::Integer(value)
    }
}

impl std::convert::TryFrom<BENValue> for isize {
    type Error = Error;

    fn try_from(value: BENValue) -> Result<isize> {
        value.int()
    }
}

impl std::convert::From<String> for BENValue {
    fn from(value: String) -> BENValue {
        BENValue::String(BytesMut::from(value))
    }
}

impl std::convert::TryFrom<BENValue> for String {
    type Error = Error;

    /// Interprets this value as a UTF-8 encoded string value.
    fn try_from(value: BENValue) -> Result<String> {
        match value {
            BENValue::String(v) => Ok(String::from_utf8(v.to_vec())?),
            _ => Err(err_msg("Not a string type value")),
        }
    }
}

impl std::convert::From<BytesMut> for BENValue {
    fn from(value: BytesMut) -> BENValue {
        BENValue::String(value)
    }
}

impl std::convert::TryFrom<BENValue> for BytesMut {
    type Error = Error;

    fn try_from(value: BENValue) -> Result<BytesMut> {
        match value {
            BENValue::String(v) => Ok(v),
            _ => Err(err_msg("Not a string type value")),
        }
    }
}

impl<T: Into<BENValue>> std::convert::From<Vec<T>> for BENValue {
    fn from(value: Vec<T>) -> BENValue {
        let mut out = vec![];
        for item in value {
            out.push(item.into());
        }

        BENValue::List(out)
    }
}

impl<T: TryFrom<BENValue, Error = Error>> TryFrom<BENValue> for Vec<T> {
    type Error = Error;

    fn try_from(value: BENValue) -> Result<Vec<T>> {
        match value {
            BENValue::List(v) => {
                let mut out = vec![];
                for value in v {
                    out.push(value.try_into()?);
                }

                Ok(out)
            }
            _ => Err(err_msg("Not a list type value")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_test() {
        assert_eq!(
            BENValue::parse(b"3:abc").unwrap(),
            (BENValue::String("abc".into()), &b""[..])
        );
        assert_eq!(
            BENValue::parse(b"l4:spam4:eggse").unwrap(),
            (
                BENValue::List(vec![
                    BENValue::String("spam".into()),
                    BENValue::String("eggs".into())
                ]),
                &b""[..]
            )
        );
        assert_eq!(
            BENValue::parse(b"d3:cow3:moo4:spam4:eggse").unwrap(),
            (
                BENValue::Dict(map!(
                    BytesMut::from("cow") => BENValue::String("moo".into()),
                    BytesMut::from("spam") => BENValue::String("eggs".into())
                )),
                &b""[..]
            )
        );

        assert_eq!(
            BENValue::parse(b"d4:spaml1:a1:bee").unwrap(),
            (
                BENValue::Dict(map!(
                    BytesMut::from("spam") => BENValue::List(vec![BENValue::String("a".into()), BENValue::String("b".into())])
                )),
                &b""[..]
            )
        );
    }

    #[test]
    fn torrent_test() {
        let data = std::fs::read(
            file::project_dir()
                .join("pkg/bittorrent/2020-08-20-raspios-buster-armhf-full.zip.torrent"),
        )
        .unwrap();
        println!("{:#?}", BENValue::parse(&data).unwrap());
        assert!(false);
    }
}
