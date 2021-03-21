#![allow(dead_code, non_snake_case)]

pub mod dsl {
    include!(concat!(env!("OUT_DIR"), "/src/proto/dsl.rs"));
}
