mod proto {
    include!(concat!(env!("OUT_DIR"), "/src/mp4.rs"));
}

pub use proto::*;