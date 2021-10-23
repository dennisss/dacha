pub mod key_value {
    include!(concat!(env!("OUT_DIR"), "/src/proto/key_value.rs"));
}

pub mod meta {
    include!(concat!(env!("OUT_DIR"), "/src/proto/meta.rs"));
}
