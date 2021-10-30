use std::collections::{HashMap, HashSet};
use std::net::SocketAddr;
use std::slice::SliceIndex;
use std::sync::Arc;

use common::async_std::sync::Mutex;
use common::async_std::task;
use common::condvar::Condvar;
use common::errors::*;
use common::vec_hash_set::VecHashSet;
use crypto::random::RngExt;

use crate::backoff::{ExponentialBackoff, ExponentialBackoffOptions};
use crate::client::client_interface::ClientInterface;
use crate::client::direct_client::DirectClient;
use crate::client::direct_client::DirectClientOptions;
use crate::client::resolver::Resolver;
use crate::request::Request;
use crate::response::Response;

#[derive(Clone)]
pub struct LoadBalancedClientOptions {
    pub resolver: Arc<dyn Resolver>,

    pub resolver_backoff: ExponentialBackoffOptions,

    /// Maximum number of distinct backends to have connected.
    ///
    /// TODO: Periodically change the subset used?
    /// TODO: Implement this.
    pub subset_size: usize,

    // TODO: need a policy for how to pick a backend.
    pub backend: DirectClientOptions,
}

#[derive(Clone)]
pub struct LoadBalancedClient {
    shared: Arc<Shared>,
}

struct Shared {
    options: LoadBalancedClientOptions,
    state: Condvar<State>,
}

struct State {
    backends: VecHashSet<SocketAddr, DirectClient>,
}

impl LoadBalancedClient {
    pub fn new(options: LoadBalancedClientOptions) -> Self {
        Self {
            shared: Arc::new(Shared {
                options,
                state: Condvar::new(State {
                    backends: VecHashSet::new(),
                }),
            }),
        }
    }

    pub async fn run(self) {
        let mut resolve_backoff =
            ExponentialBackoff::new(self.shared.options.resolver_backoff.clone());

        loop {
            if let Some(wait_time) = resolve_backoff.start_attempt() {
                task::sleep(wait_time).await;
            }

            // TODO: Retry .resolve() with backoff.
            let backend_addrs = match self.shared.options.resolver.resolve().await {
                Ok(v) => v,
                Err(e) => {
                    eprintln!("Resolver failed: {}", e);
                    resolve_backoff.end_attempt(false);
                    continue;
                }
            };

            let backend_addrs = backend_addrs.into_iter().collect::<HashSet<_>>();

            let mut add_addrs = Vec::new();
            {
                let mut state = self.shared.state.lock().await;

                let mut remove_addrs = Vec::new();
                for existing_addr in state.backends.keys() {
                    if !backend_addrs.contains(existing_addr) {
                        remove_addrs.push(*existing_addr);
                    }
                }

                // TODO: Gracefully shut down all of these.
                for addr in remove_addrs {
                    state.backends.remove(&addr);
                }

                for new_addr in &backend_addrs {
                    if !state.backends.contains_key(new_addr) {
                        add_addrs.push(*new_addr);
                    }
                }
            }

            let mut created_clients = vec![];
            for addr in add_addrs {
                let client = DirectClient::new(addr.clone(), self.shared.options.backend.clone());
                task::spawn(client.clone().run());
                created_clients.push((addr, client));
            }

            {
                let mut state = self.shared.state.lock().await;

                for (addr, client) in created_clients {
                    state.backends.insert(addr, client);
                }

                state.notify_all();
            }

            self.shared.options.resolver.wait_for_update().await;
        }
    }
}

#[async_trait]
impl ClientInterface for LoadBalancedClient {
    async fn request(&self, request: Request) -> Result<Response> {
        let client;
        loop {
            let state = self.shared.state.lock().await;
            if state.backends.values().is_empty() {
                state.wait(()).await;
                continue;
            }

            // TODO: Increment the index if we encounter a failing client.
            let mut rng = crypto::random::clocked_rng();
            client = rng.choose(state.backends.values()).clone();
            break;
        }

        client.request(request).await
    }
}
