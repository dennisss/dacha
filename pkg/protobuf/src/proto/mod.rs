#![allow(dead_code, non_snake_case)]

#[cfg(feature = "alloc")]
pub mod test {
    include!(concat!(env!("OUT_DIR"), "/src/proto/test.rs"));
}

pub mod no_alloc {
    include!(concat!(env!("OUT_DIR"), "/src/proto/no_alloc.rs"));
}
