use std::collections::HashSet;
use std::sync::Arc;

use base_error::*;

use crate::proto::*;
use crate::routing::route_resolver::RouteResolver;
use crate::routing::route_store::RouteStore;
use crate::server::channel_factory::ChannelFactory;
use crate::utils::find_peer_group_id;

pub struct RouteChannelFactory {
    group_id: GroupId,
    route_store: RouteStore,
}

impl RouteChannelFactory {
    /// Assuming there is some discovery mechanism finding new servers, this
    /// will wait for the first server group to be discovered and create a
    /// factory that connects to it.
    pub async fn find_group(route_store: RouteStore) -> Self {
        let group_id = find_peer_group_id(&route_store).await;
        Self::new(group_id, route_store)
    }

    pub fn new(group_id: GroupId, route_store: RouteStore) -> Self {
        Self {
            group_id,
            route_store,
        }
    }

    /// Creates an RPC channel which will contact any available cluster node
    /// (and may load balance different requests between any of them).
    pub async fn create_any(&self) -> Result<Arc<rpc::Http2Channel>> {
        Ok(Arc::new(
            rpc::Http2Channel::create(http::ClientOptions::from_resolver(Arc::new(
                RouteResolver::create(self.route_store.clone(), self.group_id, None),
            )))
            .await?,
        ))
    }
}

#[async_trait]
impl ChannelFactory for RouteChannelFactory {
    async fn create(&self, server_id: ServerId) -> Result<Arc<dyn rpc::Channel>> {
        Ok(Arc::new(
            rpc::Http2Channel::create(http::ClientOptions::from_resolver(Arc::new(
                RouteResolver::create(self.route_store.clone(), self.group_id, Some(server_id)),
            )))
            .await?,
        ))
    }

    async fn reachable_servers(&self) -> Result<HashSet<ServerId>> {
        let route_store = self.route_store.lock().await;
        Ok(route_store.remote_servers(self.group_id))
    }
}
