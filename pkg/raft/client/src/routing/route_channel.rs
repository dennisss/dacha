use std::collections::HashSet;
use std::convert::TryFrom;
use std::sync::Arc;

use base_error::*;
use rpc::Http2ChannelOptions;

use crate::routing::route_resolver::RouteResolver;
use crate::routing::route_store::RouteStore;
use crate::server::channel_factory::ChannelFactory;
use crate::utils::find_peer_group_id;
use crate::{proto::*, LeaderResolver};

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

    pub async fn create_leader(&self) -> Result<Arc<rpc::Http2Channel>> {
        let resolver = Arc::new(LeaderResolver::create(
            self.route_store.clone(),
            self.group_id,
        ));

        let mut options =
            Http2ChannelOptions::try_from(http::ClientOptions::from_resolver(resolver.clone()))?;

        options.http.backend_balancer.subset_size = 3;
        options.http.backend_balancer.max_backend_count = 5;
        options.response_interceptor = Some(resolver);

        // Add one extra retry opportunity since hitting a 'not leader' error is fairly
        // likely which the stub is first initialized.
        options.retrying.as_mut().unwrap().backoff.max_num_attempts += 1;

        Ok(Arc::new(rpc::Http2Channel::create(options).await?))
    }

    /// Creates an RPC channel which will contact any available cluster node
    /// (and may load balance different requests between any of them).
    pub async fn create_any(&self) -> Result<Arc<rpc::Http2Channel>> {
        let mut options = http::ClientOptions::from_resolver(Arc::new(RouteResolver::create(
            self.route_store.clone(),
            self.group_id,
            None,
        )));

        // Best to avoid having too many connections to avoid overloading servers.
        options.backend_balancer.subset_size = 2;
        options.backend_balancer.max_backend_count = 4;

        Ok(Arc::new(rpc::Http2Channel::create(options).await?))
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
