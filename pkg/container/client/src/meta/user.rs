use std::collections::HashSet;

use common::errors::*;
use container_proto::cluster::*;
use db_table::table::*;
use db_table::table_id;
use db_table::{define_singleton_table, sparse_struct};
use protobuf::{Enum, Message, StaticMessage};

pub struct UserTable {}

impl ProtobufTableTag for UserTable {
    type Message = User;

    fn table_id() -> u32 {
        table_id!(6)
    }

    fn table_name() -> &'static str {
        "User"
    }

    fn indexed_keys() -> &'static [ProtobufTableKey] {
        &[
            sparse_struct!(ProtobufTableKey {
                index_id: PRIMARY_KEY_ID,
                fields: &[ProtobufKeyField {
                    path: &[User::NAME_FIELD_NUM_RAW],
                    direction: Direction::Ascending,
                    fixed_size: false,
                }],
            })
        ]
    }
}

pub struct SessionTable {}

impl ProtobufTableTag for SessionTable {
    type Message = Session;

    fn table_id() -> u32 {
        table_id!(8)
    }

    fn table_name() -> &'static str {
        "Session"
    }

    fn indexed_keys() -> &'static [ProtobufTableKey] {
        &[
            sparse_struct!(ProtobufTableKey {
                index_id: PRIMARY_KEY_ID,
                fields: &[ProtobufKeyField {
                    path: &[Session::ID_FIELD_NUM_RAW],
                    direction: Direction::Ascending,
                    fixed_size: true,
                }],
            }),
            sparse_struct!(ProtobufTableKey {
                index_id: 1,
                index_name: Some("ByAuthKeyHash"),
                fields: &[
                    ProtobufKeyField {
                        path: &[Session::AUTH_KEY_HASH_FIELD_NUM_RAW],
                        direction: Direction::Ascending,
                        fixed_size: false,
                    },
                ],
            }),
        ]
    }
}
