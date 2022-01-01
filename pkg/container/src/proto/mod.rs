pub mod log {
    include!(concat!(env!("OUT_DIR"), "/src/proto/log.rs"));
}

pub mod config {
    include!(concat!(env!("OUT_DIR"), "/src/proto/config.rs"));
}

pub mod node_service {
    include!(concat!(env!("OUT_DIR"), "/src/proto/node_service.rs"));
}

pub mod task {
    include!(concat!(env!("OUT_DIR"), "/src/proto/task.rs"));
}

pub mod task_event {
    include!(concat!(env!("OUT_DIR"), "/src/proto/task_event.rs"));
}

pub mod job {
    include!(concat!(env!("OUT_DIR"), "/src/proto/job.rs"));
}

pub mod node {
    include!(concat!(env!("OUT_DIR"), "/src/proto/node.rs"));
}

pub mod meta {
    include!(concat!(env!("OUT_DIR"), "/src/proto/meta.rs"));
}

pub mod blob {
    include!(concat!(env!("OUT_DIR"), "/src/proto/blob.rs"));
}

pub mod manager {
    include!(concat!(env!("OUT_DIR"), "/src/proto/manager.rs"));
}
