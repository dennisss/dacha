pub mod main;
mod manager;

use cluster_client::id::{entity_id_to_string, normalize_entity_id};
use crypto::random::SharedRng;
// Mainly for use by the 'cluster' binary which uses this for bootstrapping.
pub use manager::Manager;

pub async fn new_worker_id(rng: &dyn SharedRng) -> String {
    use crypto::random::SharedRngExt;

    let id = normalize_entity_id(rng.uniform::<u64>().await);
    entity_id_to_string(id).unwrap()
}
