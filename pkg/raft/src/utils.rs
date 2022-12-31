use crypto::random::SharedRng;

use crate::proto::routing::RouteLabel;

/// Generate a unique set of route labels that are uniquely to be in use by anothe consensus instance.
/// 
/// This is mainly useful for ensuring that isolated test instances are used.
pub async fn generate_unique_route_labels() -> Vec<RouteLabel> {
    let unique_key = {
        let mut data = vec![0u8; 16];
        crypto::random::global_rng().generate_bytes(&mut data).await;
        radix::hex_encode(&data)
    };

    let mut route_label = RouteLabel::default();
    route_label.set_value(format!("INSTANCE_UUID={}", unique_key));

    vec![route_label]
}
