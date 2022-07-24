pub mod bundle {
    include!(concat!(env!("OUT_DIR"), "/src/proto/bundle.rs"));
}

pub mod config {
    include!(concat!(env!("OUT_DIR"), "/src/proto/config.rs"));
}

pub mod rule {
    include!(concat!(env!("OUT_DIR"), "/src/proto/rule.rs"));
}
