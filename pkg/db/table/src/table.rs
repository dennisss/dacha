use protobuf::{FieldNumber, StaticMessage, TypedFieldNumber};

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
    fn indexed_keys() -> &'static [ProtobufTableKey];
}

pub struct ProtobufTableKey {
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
