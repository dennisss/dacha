mod proto {
    #![allow(dead_code, non_snake_case)]
    include!(concat!(env!("OUT_DIR"), "/src/smart.rs"));
}

pub use proto::*;
