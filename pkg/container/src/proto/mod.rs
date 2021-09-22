pub mod log {
    include!(concat!(env!("OUT_DIR"), "/src/proto/log.rs"));
}

pub mod config {
    include!(concat!(env!("OUT_DIR"), "/src/proto/config.rs"));
}

pub mod service {
    include!(concat!(env!("OUT_DIR"), "/src/proto/service.rs"));
}

pub mod task {
    include!(concat!(env!("OUT_DIR"), "/src/proto/task.rs"));
}

pub mod job {
    include!(concat!(env!("OUT_DIR"), "/src/proto/job.rs"));
}

pub mod node {
    include!(concat!(env!("OUT_DIR"), "/src/proto/node.rs"));
}
