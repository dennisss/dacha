
pub mod log {
    include!(concat!(env!("OUT_DIR"), "/src/proto/log.rs"));
}

pub mod config {
    include!(concat!(env!("OUT_DIR"), "/src/proto/config.rs"));
}

pub mod service {
    include!(concat!(env!("OUT_DIR"), "/src/proto/service.rs"));
}