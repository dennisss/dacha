use automata::regexp::RegExpMatch;
use common::bytes::BytesMut;
use common::errors::*;
use parsing::*;
use std::collections::HashMap;

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
    // enum_accessor!(dict, String, String);

    pub fn parse(mut input: &[u8]) -> Result<(Self, &[u8])> {
        let m: RegExpMatch = (*TAG)
            .exec(input)?
            .ok_or_else(|| err_msg("Unknown BEN tag"))?;
        input = &input[m.last_index()..];

        Ok(if let Some(len_str) = m.group(0) {
            let len = len_str.parse::<usize>()?;
            let data = parse_next!(input, take_exact(len)).into();
            (Self::String(data), input)
        } else if let Some(int_str) = m.group(1) {
            let int = int_str.parse::<isize>()?;
            (Self::Integer(int), input)
        } else if m.group(2).is_some() {
            // List
            let items = parse_next!(input, many(Self::parse));
            parse_next!(input, tag("e"));
            (Self::List(items), input)
        } else if m.group(3).is_some() {
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
            "/home/dennis/workspace/dacha/pkg/bittorrent/2020-08-20-raspios-buster-armhf-full.zip.torrent",
        )
        .unwrap();
        println!("{:#?}", BENValue::parse(&data).unwrap());
        assert!(false);
    }
}
