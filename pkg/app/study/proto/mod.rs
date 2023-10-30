#![allow(dead_code, non_snake_case)]

pub mod card {
    include!(concat!(env!("OUT_DIR"), "/src/proto/card.rs"));
}
