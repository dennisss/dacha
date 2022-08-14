#![allow(dead_code, non_snake_case, unused_imports, unused_variables)]

pub mod net {
    include!(concat!(env!("OUT_DIR"), "/src/proto/net.rs"));
}

pub mod bootloader {
    include!(concat!(env!("OUT_DIR"), "/src/proto/bootloader.rs"));
}

pub mod log {
    include!(concat!(env!("OUT_DIR"), "/src/proto/log.rs"));
}
