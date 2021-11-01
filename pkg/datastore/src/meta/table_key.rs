use common::bytes::Bytes;
use common::errors::*;
use protobuf::{Enum, EnumValue};

use crate::key_encoding::KeyEncoder;
use crate::proto::meta::*;

/// Parsed representation of the key stored in the metastore's underlying
/// storage.
pub enum TableKey {
    UserData {
        user_key: Bytes,
        sub_key: UserDataSubKey,
    },
}

impl TableKey {
    pub fn parse(mut input: &[u8]) -> Result<Self> {
        let table_id =
            TableId::parse(parse_next!(input, KeyEncoder::decode_varuint, false) as EnumValue)?;

        match table_id {
            TableId::USER_DATA => {
                let user_key = parse_next!(input, KeyEncoder::decode_bytes);
                let sub_key =
                    UserDataSubKey::parse(
                        parse_next!(input, KeyEncoder::decode_varuint, false) as EnumValue
                    )?;
                Ok(TableKey::UserData {
                    user_key: user_key.into(),
                    sub_key,
                })
            }
            _ => {
                return Err(err_msg("Unsupported table id"));
            }
        }
    }

    pub fn serialize(&self, out: &mut Vec<u8>) {
        match self {
            TableKey::UserData { user_key, sub_key } => {
                KeyEncoder::encode_varuint(TableId::USER_DATA as u64, false, out);
                KeyEncoder::encode_bytes(user_key.as_ref(), out);
                KeyEncoder::encode_varuint(*sub_key as u64, false, out);
            }
        }
    }

    pub fn user_value(user_key: &[u8]) -> Vec<u8> {
        let mut out = vec![];
        TableKey::UserData {
            user_key: user_key.into(),
            sub_key: UserDataSubKey::USER_VALUE,
        }
        .serialize(&mut out);
        out
    }
}
