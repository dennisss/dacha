use protobuf::{FieldNumber, StaticMessage, TypedFieldNumber};

pub const PRIMARY_KEY_ID: u32 = 0;

/// Definition for a database table where each row is a protobuf (fields map to
/// columns).
pub trait ProtobufTableTag {
    type Message: StaticMessage;

    /// NOTE: The table id must be unique across all distinct tables in a
    /// database.
    fn table_id() -> u32;

    /// NOTE: The table name must be unique across all distinct tables in a
    /// database.
    fn table_name() -> &'static str;

    /// Lists all fields that are present in the primary/secondary keys.
    ///
    /// NOTE: This MUST be in sorted index_id order.
    fn indexed_keys() -> &'static [ProtobufTableKey];
}

pub struct ProtobufTableKey {
    /// Unique id for this key. Must equal PRIMARY_KEY_ID for the primary key.
    pub index_id: u32,

    /// None implies this is the primary key
    pub index_name: Option<&'static str>,

    /// Fields that are indexed/stored in this key.
    /// - For normal (non-unique) secondary keys, this should contain all the
    ///   primary key fields.
    /// - For unique indexes, this can contain zero or more of the primary key's
    ///   fields.
    pub fields: &'static [ProtobufKeyField],
}

pub struct ProtobufKeyField {
    /// TODO: Make this more type safe.
    pub path: &'static [FieldNumber],
    pub direction: Direction,
    pub fixed_size: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    Ascending,
    Descending,
}

#[macro_export]
macro_rules! define_singleton_table {
    ($t:ident { message: $msg:ty, table_id: $id:expr, table_name: $name:expr }) => {
        pub struct $t {}

        impl $crate::table::ProtobufTableTag for $t {
            type Message = $msg;

            fn table_id() -> u32 {
                $id
            }

            fn table_name() -> &'static str {
                $name
            }

            fn indexed_keys() -> &'static [$crate::table::ProtobufTableKey] {
                &[$crate::table::ProtobufTableKey {
                    index_id: $crate::table::PRIMARY_KEY_ID,
                    index_name: None,
                    fields: &[],
                }]
            }
        }
    };
}
