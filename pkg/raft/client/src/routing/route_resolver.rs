use std::convert::TryInto;
use std::sync::Arc;

use base_error::*;
use executor::child_task::ChildTask;
use executor::lock;
use executor::sync::AsyncMutex;
use http::uri::Authority;
use net::ip::SocketAddr;

use crate::proto::*;
use crate::routing::route_store::RouteStore;

pub struct RouteResolver {
    shared: Arc<Shared>,
    waiter: ChildTask,
}

struct Shared {
    route_store: RouteStore,
    group_id: GroupId,
    server_id: Option<ServerId>,
    listeners: AsyncMutex<Vec<http::ResolverChangeListener>>,
}

impl RouteResolver {
    pub fn create(route_store: RouteStore, group_id: GroupId, server_id: Option<ServerId>) -> Self {
        let shared = Arc::new(Shared {
            route_store,
            group_id,
            server_id,
            listeners: AsyncMutex::new(vec![]),
        });

        let waiter = ChildTask::spawn(Self::change_waiter(shared.clone()));

        Self { shared, waiter }
    }

    async fn change_waiter(shared: Arc<Shared>) {
        loop {
            let route_store = shared.route_store.lock().await;

            lock!(listeners <= shared.listeners.lock().await.unwrap(), {
                let mut i = 0;
                while i < listeners.len() {
                    if !(listeners[i])() {
                        let _ = listeners.swap_remove(i);
                        continue;
                    }

                    i += 1;
                }
            });

            route_store.wait().await;
        }
    }
}

#[async_trait]
impl http::Resolver for RouteResolver {
    async fn resolve(&self) -> Result<Vec<http::ResolvedEndpoint>> {
        let mut route_store = self.shared.route_store.lock().await;

        let mut server_ids = vec![];
        if let Some(id) = &self.shared.server_id {
            server_ids.push(*id);
        } else {
            for id in route_store.remote_servers(self.shared.group_id) {
                server_ids.push(id);
            }
        }

        let mut endpoints = vec![];

        for id in server_ids {
            let route = match route_store.lookup(self.shared.group_id, id) {
                Some(id) => id,
                None => {
                    // This will only happen if we are resolving a specific server id and that id
                    // isn't in the route store yet.
                    continue;
                }
            };

            let authority = route.target().addr().parse::<Authority>()?;
            let ip = match &authority.host {
                http::uri::Host::IP(ip) => ip.clone(),
                _ => {
                    return Err(err_msg("Route doesn't contain an ip address"));
                }
            };

            let port = authority.port.ok_or_else(|| err_msg("No port in route"))?;

            let address = SocketAddr::new(ip, port);

            endpoints.push(http::ResolvedEndpoint { address, authority });
        }

        Ok(endpoints)
    }

    async fn add_change_listener(&self, listener: http::ResolverChangeListener) {
        lock!(listeners <= self.shared.listeners.lock().await.unwrap(), {
            listeners.push(listener);
        });
    }
}
