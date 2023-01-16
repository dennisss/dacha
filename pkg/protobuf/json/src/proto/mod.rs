#![allow(dead_code, non_snake_case)]

pub mod test {
    include!(concat!(env!("OUT_DIR"), "/src/proto/test.rs"));
}
