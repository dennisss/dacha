#![allow(dead_code, non_snake_case, unused_imports, unused_variables)]

pub mod bridge {
    include!(concat!(env!("OUT_DIR"), "/src/proto/bridge.rs"));
}
