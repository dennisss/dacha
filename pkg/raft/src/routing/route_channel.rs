use std::collections::HashSet;
use std::sync::Arc;

use common::async_std::sync::Mutex;
use common::errors::*;
use rpc::Channel;

use crate::proto::consensus::ServerId;
use crate::proto::server_metadata::GroupId;
use crate::routing::route_store::RouteStoreHandle;
use crate::server::channel_factory::ChannelFactory;

pub struct RouteChannel {
    group_id: GroupId,
    server_id: ServerId,
    route_store: RouteStoreHandle,
    current_channel: Mutex<Option<CurrentChannel>>,
}

struct CurrentChannel {
    channel: rpc::Http2Channel,
    addr: String,
}

impl RouteChannel {
    pub fn new(group_id: GroupId, server_id: ServerId, route_store: RouteStoreHandle) -> Self {
        Self {
            group_id,
            server_id,
            route_store,
            current_channel: Mutex::new(None),
        }
    }

    async fn call_raw_impl(
        &self,
        service_name: &str,
        method_name: &str,
        request_context: &rpc::ClientRequestContext,
    ) -> Result<(
        rpc::ClientStreamingRequest<()>,
        rpc::ClientStreamingResponse<()>,
    )> {
        let mut route_store = self.route_store.lock().await;
        let mut current_channel = self.current_channel.lock().await;

        let latest_route = route_store.lookup(self.group_id, self.server_id);
        if let Some(route) = latest_route {
            let current_channel_valid = match current_channel.as_ref() {
                Some(channel) => channel.addr == route.addr(),
                None => false,
            };

            if !current_channel_valid {
                *current_channel = Some(CurrentChannel {
                    addr: route.addr().to_string(),
                    channel: rpc::Http2Channel::create(http::ClientOptions::from_uri(
                        &route.addr().parse::<http::uri::Uri>()?,
                    )?)?,
                });
            }
        }

        // TODO: We need a good mechanism for retrying this (especially based on
        // feedback from RouteChannel changes).
        let channel = current_channel
            .as_ref()
            .ok_or_else(|| rpc::Status::cancelled("No route to server"))?;

        // NOTE: rpc::Http2Channel has a cheap call_raw with most of the logic happening
        // asynchronously so there is no point in unlocking the 'channel' before
        // performing the call.
        Ok(channel
            .channel
            .call_raw(service_name, method_name, request_context)
            .await)
    }
}

#[async_trait]
impl rpc::Channel for RouteChannel {
    async fn call_raw(
        &self,
        service_name: &str,
        method_name: &str,
        request_context: &rpc::ClientRequestContext,
    ) -> (
        rpc::ClientStreamingRequest<()>,
        rpc::ClientStreamingResponse<()>,
    ) {
        match self
            .call_raw_impl(service_name, method_name, request_context)
            .await
        {
            Ok(v) => v,
            Err(e) => (
                rpc::ClientStreamingRequest::closed(),
                rpc::ClientStreamingResponse::from_error(e),
            ),
        }
    }
}

pub struct RouteChannelFactory {
    group_id: GroupId,
    route_store: RouteStoreHandle,
}

impl RouteChannelFactory {
    pub fn new(group_id: GroupId, route_store: RouteStoreHandle) -> Self {
        Self {
            group_id,
            route_store,
        }
    }
}

#[async_trait]
impl ChannelFactory for RouteChannelFactory {
    async fn create(&self, server_id: ServerId) -> Result<Arc<dyn rpc::Channel>> {
        Ok(Arc::new(RouteChannel::new(
            self.group_id,
            server_id,
            self.route_store.clone(),
        )))
    }

    async fn reachable_servers(&self) -> Result<HashSet<ServerId>> {
        let route_store = self.route_store.lock().await;
        Ok(route_store.remote_servers(self.group_id))
    }
}
