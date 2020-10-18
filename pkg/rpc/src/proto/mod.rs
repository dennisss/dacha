#![allow(dead_code, non_snake_case)]

pub mod adder {
    include!(concat!(env!("OUT_DIR"), "/src/proto/adder.rs"));
}
