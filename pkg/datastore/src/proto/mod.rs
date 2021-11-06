pub mod key_value {
    include!(concat!(env!("OUT_DIR"), "/src/proto/key_value.rs"));
}

pub mod meta {
    include!(concat!(env!("OUT_DIR"), "/src/proto/meta.rs"));
}

pub mod client {
    include!(concat!(env!("OUT_DIR"), "/src/proto/client.rs"));
}

pub mod lock {
    include!(concat!(env!("OUT_DIR"), "/src/proto/lock.rs"));
}
