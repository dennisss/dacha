#![allow(dead_code, non_snake_case)]

pub mod timestamp {
    include!(concat!(env!("OUT_DIR"), "/src/proto/timestamp.rs"));

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
}

pub mod any {
    include!(concat!(env!("OUT_DIR"), "/src/proto/any.rs"));

    impl Any {
        pub fn unpack<T: protobuf::Message + Default>(&self) -> Result<Option<T>> {
            let mut v = T::default();
            if self.type_url() != v.type_url() {
                return Ok(None);
            }

            v.parse_merge(self.value())?;
            Ok(Some(v))
        }

        pub fn pack_from<M: protobuf::Message>(&mut self, message: &M) -> Result<()> {
            self.set_type_url(message.type_url());
            self.set_value(message.serialize()?);
            Ok(())
        }
    }
}

pub mod rpc {
    include!(concat!(env!("OUT_DIR"), "/src/proto/rpc.rs"));
}

pub mod code {
    include!(concat!(env!("OUT_DIR"), "/src/proto/code.rs"));
}

pub mod wrappers {
    include!(concat!(env!("OUT_DIR"), "/src/proto/wrappers.rs"));
}

pub mod empty {
    include!(concat!(env!("OUT_DIR"), "/src/proto/empty.rs"));
}

pub mod duration {
    include!(concat!(env!("OUT_DIR"), "/src/proto/duration.rs"));
}

pub mod profile {
    include!(concat!(env!("OUT_DIR"), "/src/proto/profile.rs"));
}
