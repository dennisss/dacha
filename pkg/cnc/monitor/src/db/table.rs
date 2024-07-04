use protobuf::{StaticMessage, TypedFieldNumber};

pub trait ProtobufTableTag {
    type Message: StaticMessage;

    fn table_id() -> u32;

    fn table_name() -> &'static str;

    /// Lists all fields that are present in the primary/secondary keys.
    fn indexed_keys() -> &'static [ProtobufTableKey<Self::Message>];
}

pub struct ProtobufTableKey<T: 'static> {
    /// None implies this is the primary key
    pub index_name: Option<&'static str>,

    /// Fields that are indexed/stored in this key.
    /// - For normal (non-unique) secondary keys, this should contain all the
    ///   primary key fields.
    /// - For unique indexes, this can contain zero or more of the primary key's
    ///   fields.
    pub fields: &'static [ProtobufKeyField<T>],
}

pub struct ProtobufKeyField<T> {
    pub number: TypedFieldNumber<T>,
    pub direction: Direction,
    pub fixed_size: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    Ascending,
    Descending,
}
