use std::collections::HashSet;
use std::sync::Arc;

use common::errors::*;

use crate::proto::consensus::ServerId;
use crate::proto::server_metadata::GroupId;
use crate::routing::route_resolver::RouteResolver;
use crate::routing::route_store::RouteStore;
use crate::server::channel_factory::ChannelFactory;

pub struct RouteChannelFactory {
    group_id: GroupId,
    route_store: RouteStore,
}

impl RouteChannelFactory {
    /// Assuming there is some discovery mechanism finding new servers, this
    /// will wait for the first server group to be discovered and create a
    /// factory that connects to it.
    pub async fn find_group(route_store: RouteStore) -> Self {
        let group_id = crate::node::Node::<()>::find_peer_group_id(&route_store).await;

        Self {
            group_id,
            route_store,
        }
    }

    pub fn new(group_id: GroupId, route_store: RouteStore) -> Self {
        Self {
            group_id,
            route_store,
        }
    }

    /// Creates a channel which will contact any available cluster node (and may
    /// load balance different rquests between any of them).
    pub fn create_any(&self) -> Result<Arc<dyn rpc::Channel>> {
        Ok(Arc::new(rpc::Http2Channel::create(
            http::ClientOptions::from_resolver(Arc::new(RouteResolver::create(
                self.route_store.clone(),
                self.group_id,
                None,
            ))),
        )?))
    }
}

#[async_trait]
impl ChannelFactory for RouteChannelFactory {
    async fn create(&self, server_id: ServerId) -> Result<Arc<dyn rpc::Channel>> {
        Ok(Arc::new(rpc::Http2Channel::create(
            http::ClientOptions::from_resolver(Arc::new(RouteResolver::create(
                self.route_store.clone(),
                self.group_id,
                Some(server_id),
            ))),
        )?))
    }

    async fn reachable_servers(&self) -> Result<HashSet<ServerId>> {
        let route_store = self.route_store.lock().await;
        Ok(route_store.remote_servers(self.group_id))
    }
}
