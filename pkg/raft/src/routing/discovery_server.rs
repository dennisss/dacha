use std::time::SystemTime;

use common::errors::*;

use crate::proto::routing::*;
use crate::routing::route_store::*;

pub struct DiscoveryServer {
    route_store: RouteStoreHandle,
}

impl DiscoveryServer {
    pub fn new(route_store: RouteStoreHandle) -> Self {
        Self { route_store }
    }
}

#[async_trait]
impl DiscoveryService for DiscoveryServer {
    async fn Announce(
        &self,
        request: rpc::ServerRequest<Announcement>,
        response: &mut rpc::ServerResponse<Announcement>,
    ) -> Result<()> {
        let mut route_store = self.route_store.lock().await;

        // TODO: Ignore remote last_used
        route_store.apply(&request);

        // TODO: Don't need to send back any routes we just got from the remote client.
        response.value = route_store.serialize();

        Ok(())
    }
}
