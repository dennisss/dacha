#![allow(dead_code, non_snake_case)]

pub mod data {
    include!(concat!(env!("OUT_DIR"), "/src/proto/data.rs"));
}
