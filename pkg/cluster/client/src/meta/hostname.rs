use raft_client::RouteHostnameResolver;

use crate::{id::entity_id_to_string, service::address::ServiceName};

/// Server id used for the first server started in the metastore for
/// bootstrapping purposes.
pub const ROOT_SERVER_ID: u64 = 1;

/// A resolver which finds the address of the metastore in a cluster. 
pub struct ClusterMetaHostnameResolver {
    zone: String,
}

impl ClusterMetaHostnameResolver {
    pub fn new(zone: &str) -> Self {
        Self {
            zone: zone.to_string(),
        }
    }
}

impl RouteHostnameResolver for ClusterMetaHostnameResolver {
    fn route_hostname(&self, route: &raft_client::proto::Route) -> Option<String> {
        if route.server_id().value() == ROOT_SERVER_ID {
            return Some(ServiceName::for_root(&self.zone).unwrap().to_string());
        }

        let id = match entity_id_to_string(route.server_id().value()) {
            Some(v) => v,
            None => return None,
        };

        let name = match ServiceName::for_worker(&self.zone, &format!("system.meta.{}", id)) {
            Ok(v) => v,
            Err(_) => return None,
        };

        Some(name.to_string())
    }

    fn anonymous_route_hostname(&self) -> String {
        ServiceName::for_job(&self.zone, "system.meta")
            .unwrap()
            .to_string()
    }
}
