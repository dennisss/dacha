use crypto::random::SharedRng;
use raft_proto::raft::GroupId;

use crate::{proto::RouteLabel, RouteStore};

/// Generate a unique set of route labels that are uniquely to be in use by
/// another consensus instance.
///
/// This is mainly useful for ensuring that isolated test instances are used.
pub async fn generate_unique_route_labels() -> Vec<RouteLabel> {
    let unique_key = {
        let mut data = vec![0u8; 16];
        crypto::random::global_rng().generate_bytes(&mut data).await;
        base_radix::hex_encode(&data)
    };

    let mut route_label = RouteLabel::default();
    route_label.set_value(format!("INSTANCE_UUID={}", unique_key));

    vec![route_label]
}

// TODO: Ensure this is only used by this crate and the 'raft' crate.
pub async fn find_peer_group_id(route_store: &RouteStore) -> GroupId {
    loop {
        let route_store = route_store.lock().await;

        let remote_groups = route_store.remote_groups();

        if remote_groups.is_empty() {
            route_store.wait().await;
            continue;
        }

        drop(route_store);

        return *remote_groups.iter().next().unwrap();
    }
}
