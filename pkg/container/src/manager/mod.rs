pub mod main;
mod manager;

// Mainly for use by the 'cluster' binary which uses this for bootstrapping.
pub use manager::Manager;

pub fn new_worker_id() -> String {
    use crypto::random::RngExt;

    let id = crypto::random::clocked_rng().uniform::<u64>();
    radix::base32_encode_cl64(id)
}
