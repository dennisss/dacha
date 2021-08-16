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

/*

Bootstraping a cluster:
- Start one node
    - Bootstrap the id to be 1.
- When a task is started, the node will provide the following variables:
    -
- Manually create a metadata store task
    - This will require making an adhoc selection of a port
- Populate the metadata store with:
    - 1 node entry.
    - 1 job entry for the metadata-store
    - 1 task entry for the metadata-store
- When
    -
    -


What does the manager need to know:
- IP addresses of metadata server replicas
- The manager will have a local storage disk which contains a name to ip address:port cache
  that it can use for finding metadata servers (at least one of them must be reachable and then
  we can regenerate the cache).
- Alternatively, we could have a DNS service running on each node
- When a service wants to find a location, it



CONTAINER_NODE_ID=XX
CONTAINER_NAME_SERVER=127.0.0.1:30001
    - DNS code needs to know where the metadata-store is located
    - Once we know that, we can query the metadata-store to get more


Port range to use:
    - Same as kubernetes: 30000-32767 per node
*/
