use macros::ConstDefault;
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

    /// If true, then this table will ALWAYS only have a single index (the
    /// primary key).
    ///
    /// This is a requirement for per-row ACLs since we can't easily ACL
    /// restrict indexes.
    fn single_index_table() -> bool {
        false
    }

    /// Lists all fields that are present in the primary/secondary keys.
    ///
    /// NOTE: This MUST be in sorted index_id order.
    fn indexed_keys() -> &'static [ProtobufTableKey];
}

#[derive(Debug, ConstDefault)]
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

    /// Query expression which must evaluate to true on a row for it to be
    /// included in this index.
    ///
    /// If a field's value is implied by the filter, then it shouln't be
    /// included in the 'fields'. TODO: Validate this.
    ///
    /// NOTE: No filter is allowed on the primary key.
    pub filter: Option<&'static str>,

    /// If true, this index will permanently only contain a single column
    /// family.
    pub single_column_family: bool,
}

#[derive(Debug)]
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
                    single_column_family: false,
                    filter: None,
                    fields: &[],
                }]
            }
        }
    };
}

#[macro_export]
macro_rules! sparse_struct {
    ($name:ty { $( $field:ident : $v:expr ),* $(,)? }) => {{
        const VALUE: $name = {
            let mut s = <$name as $crate::common::const_default::ConstDefault>::DEFAULT;
            $(
                s.$field = $v;
            )*
            s
        };

        VALUE
    }};
}
