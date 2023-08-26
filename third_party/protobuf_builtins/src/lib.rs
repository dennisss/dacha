//
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
