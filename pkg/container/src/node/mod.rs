mod blob_store;
pub mod main;
pub mod node;
mod resources;
pub mod shadow;
mod worker;
mod workers_table;

// Move these into sub-modules.

/*
Important invariants to test:
- Must not set NodeMetadata::last_seen before we update the WorkerStateMetadata for all workers on this node.
- If a worker reaches the DONE state, save that into our local metadata and ensure we continue to update the WorkerStateMetadata with that on future restarts (rather than re-starting the worker).

How to have a client connect to a job:
- Initially query all the workers and NodeMetadata
- If a connection to a node fails, then double check the NodeMetadata.
- Otherwise just keep on going.
- So, we don't really on
    ^ Does not require watching the NodeMetadata

TODO: Verify that sending a kill to the runtime doesn't cause an error if the container just recently died and we didn't process the event notification yet.

Usage of the WorkerMetadata in the local node db:
- When a worker is started, we record it as STARTED
    - This has the main purpose of

TODO: Because workers can reach WorkerStateMetadata::STOPPED on old revisions, we can't reliably tell when a new worker revision reaches that state the second time.

*/
