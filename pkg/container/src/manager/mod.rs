pub mod main;
mod manager;

// Mainly for use by the 'cluster' binary which uses this for bootstrapping.
pub use manager::Manager;

pub fn new_task_id() -> String {
    use crypto::random::RngExt;

    let id = crypto::random::clocked_rng().uniform::<u64>();
    common::base32::base32_encode_cl64(id)
}
