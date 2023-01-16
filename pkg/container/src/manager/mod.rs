pub mod main;
mod manager;

use crypto::random::SharedRng;
// Mainly for use by the 'cluster' binary which uses this for bootstrapping.
pub use manager::Manager;

pub async fn new_worker_id(rng: &dyn SharedRng) -> String {
    use crypto::random::SharedRngExt;

    let id = rng.uniform::<u64>().await;
    base_radix::base32_encode_cl64(id)
}
