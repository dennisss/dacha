#![no_std]

#[cfg(feature = "std")]
#[macro_use]
extern crate std;

#[cfg(feature = "alloc")]
extern crate alloc;

extern crate common;
extern crate protobuf_core;
#[macro_use]
extern crate macros;

include!(concat!(env!("OUT_DIR"), "/proto_lib.rs"));

use common::errors::*;
use google::protobuf::Duration;
use google::protobuf::Timestamp;

impl std::convert::From<std::time::SystemTime> for Timestamp {
    fn from(time: std::time::SystemTime) -> Self {
        (&time).into()
    }
}

impl std::convert::From<&std::time::SystemTime> for Timestamp {
    fn from(time: &std::time::SystemTime) -> Self {
        let dur = time.duration_since(std::time::UNIX_EPOCH).unwrap();

        let mut inst = Self::default();
        inst.set_seconds(dur.as_secs() as i64);
        inst.set_nanos(dur.subsec_nanos() as i32);
        inst
    }
}

impl std::convert::From<&Timestamp> for std::time::SystemTime {
    fn from(v: &Timestamp) -> std::time::SystemTime {
        std::time::UNIX_EPOCH
            + std::time::Duration::from_secs(v.seconds() as u64)
            + std::time::Duration::from_nanos(v.nanos() as u64)
    }
}

impl std::convert::From<std::time::Duration> for Duration {
    fn from(value: std::time::Duration) -> Self {
        (&value).into()
    }
}

impl std::convert::From<&std::time::Duration> for Duration {
    fn from(value: &std::time::Duration) -> Self {
        let mut inst = Self::default();
        inst.set_seconds(value.as_secs() as i64);
        inst.set_nanos(value.subsec_nanos() as i32);
        inst
    }
}

impl std::convert::From<&Duration> for std::time::Duration {
    fn from(value: &Duration) -> Self {
        // NOTE: We can't represent negative durations using the standard duration type
        // so that will overflow.
        std::time::Duration::from_secs(value.seconds() as u64)
            + std::time::Duration::from_nanos(value.nanos() as u64)
    }
}

use google::protobuf::Any;

impl Any {
    // pub fn unpack_to<M: protobuf_core::Message>(&self, message: &mut M) ->
    // Result<()> {

    // }

    pub fn unpack<M: protobuf_core::Message + Default>(&self) -> Result<Option<M>> {
        let mut v = M::default();
        if self.type_url() != v.type_url() {
            return Ok(None);
        }

        v.parse_merge(self.value())?;
        Ok(Some(v))
    }

    pub fn pack_from<M: protobuf_core::Message>(&mut self, message: &M) -> Result<()> {
        self.set_type_url(message.type_url());
        self.set_value(message.serialize()?);
        Ok(())
    }
}

pub trait ToAnyProto {
    fn to_any_proto(&self) -> Result<Any>;
}

impl<M: protobuf_core::Message> ToAnyProto for M {
    fn to_any_proto(&self) -> Result<Any> {
        let mut any = Any::default();
        any.pack_from(self)?;
        Ok(any)
    }
}
