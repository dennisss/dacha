mod descriptors {
    #![allow(dead_code, non_snake_case, non_camel_case_types)]

    include!(concat!(env!("OUT_DIR"), "/src/proto/descriptors.rs"));
}

pub use descriptors::*;