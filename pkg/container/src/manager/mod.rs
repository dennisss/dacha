pub mod main;
mod manager;

// Mainly for use by the 'cluster' binary which uses this for bootstrapping.
pub use manager::Manager;

pub fn new_task_id() -> String {
    use crypto::random::RngExt;

    let id = crypto::random::clocked_rng().uniform::<u64>();
    format!("{:08x}", id)
}
