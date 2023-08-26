use common::bytes::Bytes;
use common::errors::*;
use protobuf::{Enum, EnumValue};

use crate::key_encoding::KeyEncoder;
use crate::proto::*;

/// Parsed representation of the key stored in the metastore's underlying
/// storage.
pub enum TableKey {
    UserData {
        user_key: Bytes,
        sub_key: UserDataSubKey,
    },

    /// Singleton row which contains the date/time at which all the writes with
    /// the same sequence number were written.
    TransactionTime,
}

impl TableKey {
    pub fn parse(mut input: &[u8]) -> Result<Self> {
        let table_id =
            TableId::parse(parse_next!(input, KeyEncoder::decode_varuint, false) as EnumValue)?;

        let key = match table_id {
            TableId::USER_DATA => {
                let user_key = parse_next!(input, KeyEncoder::decode_bytes);
                let sub_key =
                    UserDataSubKey::parse(
                        parse_next!(input, KeyEncoder::decode_varuint, false) as EnumValue
                    )?;
                TableKey::UserData {
                    user_key: user_key.into(),
                    sub_key,
                }
            }
            TableId::TRANSACTION_TIME => TableKey::TransactionTime,
            _ => {
                return Err(err_msg("Unsupported table id"));
            }
        };

        if !input.is_empty() {
            return Err(err_msg("Extra bytes left at end of key"));
        }

        Ok(key)
    }

    pub fn serialize(&self, out: &mut Vec<u8>) {
        match self {
            TableKey::UserData { user_key, sub_key } => {
                KeyEncoder::encode_varuint(TableId::USER_DATA as u64, false, out);
                KeyEncoder::encode_bytes(user_key.as_ref(), out);
                KeyEncoder::encode_varuint(*sub_key as u64, false, out);
            }

            TableKey::TransactionTime => {
                KeyEncoder::encode_varuint(TableId::TRANSACTION_TIME as u64, false, out);
            }
        }
    }

    pub fn transaction_time() -> Vec<u8> {
        let mut out = vec![];
        TableKey::TransactionTime.serialize(&mut out);
        out
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
