use std::collections::HashMap;

use common::errors::*;
use common::{async_std::fs::File, futures::AsyncReadExt};

use crate::nist::response_syntax::*;

/// NOTE: All the keys are normalized to be uppercase.
///
/// TODO: Verify that all fields and attributes are always used.
#[derive(Debug, PartialEq)]
pub struct ResponseBlock {
    pub new_attributes: bool,
    pub attributes: HashMap<String, String>,
    pub fields: HashMap<String, String>,
}

impl ResponseBlock {
    pub fn binary_field(&self, name: &str) -> Result<Vec<u8>> {
        Ok(common::hex::decode(
            &self
                .fields
                .get(name)
                .ok_or_else(|| format_err!("No field name: {}", name))?,
        )?)
    }
}

/// A Response (.rsp) file typically containing test vectors.
pub struct ResponseFile {
    data: String,
}

impl ResponseFile {
    pub async fn open<P: AsRef<std::path::Path>>(path: P) -> Result<Self> {
        let mut data = String::new();

        let mut file = File::open(path.as_ref()).await?;
        file.read_to_string(&mut data).await?;

        Ok(Self { data })
    }

    pub fn from(data: String) -> Self {
        Self { data }
    }

    pub fn iter(&self) -> ResponseBlockIter {
        ResponseBlockIter {
            remaining_data: &self.data,
            next_element: None,
            last_attributes: HashMap::new(),
        }
    }
}

pub struct ResponseBlockIter<'a> {
    remaining_data: &'a str,
    next_element: Option<Element<'a>>,
    last_attributes: HashMap<String, String>,
}

impl<'a> std::iter::Iterator for ResponseBlockIter<'a> {
    type Item = Result<ResponseBlock>;

    fn next(&mut self) -> Option<Self::Item> {
        let mut fields = HashMap::new();
        let mut cleared_attrs = false;

        while self.remaining_data.len() > 0 {
            let el = {
                if let Some(e) = self.next_element.take() {
                    e
                } else {
                    let (e, rest) = match Element::parse_next(self.remaining_data) {
                        Ok(v) => v,
                        Err(err) => {
                            return Some(Err(err));
                        }
                    };
                    self.remaining_data = rest;
                    e
                }
            };

            match el {
                Element::Field { key, value } => {
                    // TODO: Check no duplicates.
                    fields.insert(key.to_ascii_uppercase(), value.unwrap_or("").to_string());
                }
                Element::Attribute { key, value } => {
                    if fields.is_empty() {
                        if !cleared_attrs {
                            self.last_attributes.clear();
                            cleared_attrs = true;
                        }

                        // TODO: Check no duplicates.
                        self.last_attributes
                            .insert(key.to_ascii_uppercase(), value.unwrap_or("").to_string());
                    } else {
                        self.next_element = Some(el);
                        break;
                    }
                }
                Element::EndOfLine => {
                    // TODO: Currently this applies that a comment on a line by itself can delimit
                    // two responses. We should check if this is acceptable.
                    if !fields.is_empty() {
                        break;
                    }
                }
            }
        }

        if fields.is_empty() {
            return None;
        }

        Some(Ok(ResponseBlock {
            new_attributes: cleared_attrs,
            attributes: self.last_attributes.clone(),
            fields,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn response_file_test() -> Result<()> {
        let file = ResponseFile::from(
            r#"# This is a comment
# And another comment

[Apples = 12]
[Oranges = 4]

Count = 0
Key = 123456
Value = 890371

Count = 1
Hello = World
Red = Black
FAIL

[ENCRYPT]

Empty = 
EmptySpace = 
Oranges = Tasty"#
                .to_string(),
        );

        let mut iter = file.iter();

        assert_eq!(
            iter.next().unwrap().unwrap(),
            ResponseBlock {
                new_attributes: true,
                attributes: map! {
                    "APPLES" => "12",
                    "ORANGES" => "4"
                },
                fields: map! {
                    "COUNT" => "0",
                    "KEY" => "123456",
                    "VALUE" => "890371"
                }
            }
        );

        assert_eq!(
            iter.next().unwrap().unwrap(),
            ResponseBlock {
                new_attributes: false,
                attributes: map! {
                    "APPLES" => "12",
                    "ORANGES" => "4"
                },
                fields: map! {
                    "COUNT" => "1",
                    "HELLO" => "World",
                    "RED" => "Black",
                    "FAIL" => ""
                }
            }
        );

        assert_eq!(
            iter.next().unwrap().unwrap(),
            ResponseBlock {
                new_attributes: true,
                attributes: map! {
                    "ENCRYPT" => ""
                },
                fields: map! {
                    "EMPTY" => "",
                    "EMPTYSPACE" => "",
                    "ORANGES" => "Tasty"
                }
            }
        );

        assert!(iter.next().is_none());

        Ok(())
    }
}

/*
# CAVS 14.0
# GCM Encrypt with keysize 128 test information
# Generated on Fri Aug 31 11:23:06 2012



[Keylen = 128]
[IVlen = 96]
[PTlen = 0]
[AADlen = 0]
[Taglen = 128]

Count = 0
Key = 11754cd72aec309bf52f7687212e8957
IV = 3c819d9a9bed087615030b65
PT =
AAD =
CT =
Tag = 250327c674aaf477aef2675748cf6971


[ENCRYPT]

COUNT = 0
KEY = dea64f83cfe6a0a183ddbe865cfca059b3c615c1623d63fc
IV = 426fbc087b50b395c0fc81ef9fd6d1aa
PLAINTEXT = cd0b8c8a8179ecb171b64c894a4d60fd
CIPHERTEXT = ae6302d22da9458117f5681431fc80df


*/
