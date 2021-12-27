#![allow(dead_code, non_snake_case, unused_imports, unused_variables)]

pub mod protocol {
    include!(concat!(env!("OUT_DIR"), "/src/proto/protocol.rs"));
}
