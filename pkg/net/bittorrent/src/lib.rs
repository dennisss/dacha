extern crate alloc;
extern crate core;

#[macro_use]
extern crate automata;
#[macro_use]
extern crate common;
#[macro_use]
extern crate parsing;
#[macro_use]
extern crate regexp_macros;

pub mod ben;

use std::collections::HashMap;
use std::convert::TryInto;

use common::bytes::BytesMut;
use common::errors::*;
use http::query::QueryParamsBuilder;
use parsing::ascii::AsciiString;

use crate::ben::BENValue;

// Helper for taking a value from a HashMap with a given key.
// Returns an error if the key is missing.
macro_rules! take_key {
    ($dict:expr, $key:literal) => {
        $dict.remove($key.as_bytes())
            .ok_or_else(|| err_msg(stringify!(Missing required key: $key)))?
    };
}

macro_rules! ben_field_type {
    (required, $typ:ty) => {
        $typ
    };
    (optional, $typ:ty) => {
        Option<$typ>
    };
}

macro_rules! ben_try_from_dict {
    ($dict:ident, $key:expr, optional, $typ:ty) => {
        if let Some(value) = $dict.remove($key.as_bytes()) {
            Some(value.try_into()?)
        } else {
            None
        }
    };
    ($dict:ident, $key:expr, required, $typ:ty) => {
        take_key!($dict, $key).try_into()?
    };
}

macro_rules! ben_into_dict {
    ($dict:ident, $key:expr, optional, $value:expr) => {
        if let Some(v) = $value {
            ben_into_dict!($dict, $key, required, v);
        }
    };
    ($dict:ident, $key:expr, required, $value:expr) => {
        // TODO: Verify no duplicates with unknown_fields.
        $dict.insert(BytesMut::from($key), $value.into());
    };
}

macro_rules! ben_dict {
    ($(#[$meta:meta])* pub struct $name:ident { $( $(#[$field_meta:meta])* $field:ident ($key:literal): $presence:ident $typ:ty ),* }) => {
        $(#[$meta])*
        #[derive(Debug, Clone)]
        pub struct $name {
            $(
                $(#[$field_meta])*
                pub $field: ben_field_type!($presence, $typ),
            )*

            pub unknown_fields: HashMap<BytesMut, BENValue>
        }

        impl ::std::convert::TryFrom<BENValue> for $name {
            type Error = ::common::errors::Error;

            fn try_from(value: BENValue) -> Result<Self> {
                let mut dict = value.dict()?;

                $(
                    let $field = ben_try_from_dict!(dict, $key, $presence, $typ);
                )*

                Ok(Self {
                    $(
                        $field,
                    )*
                    unknown_fields: dict
                })
            }
        }

        impl ::std::convert::Into<BENValue> for $name {
            fn into(self) -> BENValue {
                let mut fields = self.unknown_fields;

                $(
                    ben_into_dict!(fields, $key, $presence, self.$field);
                )*

                BENValue::Dict(fields)
            }
        }
    };
}

ben_dict!(
    /// Aka a '.torrent' file.
    pub struct Metainfo {
        announce ("announce"): required String,
        info ("info"): required MetainfoInfo
    }
);

impl Metainfo {
    pub fn parse(input: &[u8]) -> Result<Self> {
        let root_value: BENValue = parsing::complete(BENValue::parse)(input)?.0;
        root_value.try_into()
    }
}

ben_dict!(
    pub struct MetainfoInfo {
        name ("name"): required String,
        piece_length ("piece length"): required isize,

        /// TODO: Verify that the length of this is a multiple of 20.
        pieces ("pieces"): required BytesMut,

        /// If present, then this struct represents a single file with this length.
        /// MUST not be present if 'files' is present.
        length ("length"): optional isize,

        /// If present, then this struct represents multiple files.
        /// MUST not be present if 'length' is present.
        files ("files"): optional Vec<MetainfoFile>
    }
);

ben_dict!(
    pub struct MetainfoFile {
        /// TODO: Change to u64 with negative validation.
        length ("length"): required isize,
        path ("path"): required Vec<String>
    }
);

/// Encoded in a GET request to the tracker.
#[derive(Debug)]
pub struct TrackerRequest {
    /// NOTE: Always 20 bytes long.
    pub info_hash: Vec<u8>,
    /// NOTE: Always 20 bytes long
    pub peer_id: Vec<u8>,
    pub ip: Option<String>,
    pub port: u16,
    pub uploaded: u64,
    pub downloaded: u64,
    pub left: u64,
    pub event: String, // "empty"
}

impl TrackerRequest {
    pub fn to_query_string(&self) -> AsciiString {
        let mut qs = QueryParamsBuilder::new();

        qs.add(b"info_hash", self.info_hash.as_ref());
        qs.add(b"peer_id", self.peer_id.as_ref());

        if let Some(ip) = &self.ip {
            qs.add(b"ip", ip.as_bytes());
        }

        qs.add(b"port", self.port.to_string().as_bytes());
        qs.add(b"uploaded", self.uploaded.to_string().as_bytes());
        qs.add(b"downloaded", self.downloaded.to_string().as_bytes());
        qs.add(b"left", self.left.to_string().as_bytes());
        qs.add(b"event", self.event.as_bytes());

        qs.build()
    }
}

#[derive(Debug)]
struct TrackerResponse {
    failure_reason: Option<String>,
}
