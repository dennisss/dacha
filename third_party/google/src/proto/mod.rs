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
            std::time::UNIX_EPOCH +
            std::time::Duration::from_secs(v.seconds() as u64) + 
            std::time::Duration::from_nanos(v.nanos() as u64)
        }
    }
}