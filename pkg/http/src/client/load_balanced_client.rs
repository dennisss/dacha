use std::collections::{HashMap, HashSet};
use std::net::SocketAddr;
use std::slice::SliceIndex;
use std::sync::Arc;

use common::async_std::channel;
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
use crate::client::resolver::{ResolvedEndpoint, Resolver};
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
    backends: VecHashSet<ResolvedEndpoint, DirectClient>,
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

        let resolver_listener = {
            let (sender, receiver) = channel::bounded(1);
            self.shared
                .options
                .resolver
                .add_change_listener(Box::new(move || match sender.try_send(()) {
                    Ok(()) => true,
                    Err(channel::TrySendError::Full(_)) => true,
                    Err(channel::TrySendError::Closed(_)) => false,
                }))
                .await;

            receiver
        };

        loop {
            if let Some(wait_time) = resolve_backoff.start_attempt() {
                task::sleep(wait_time).await;
            }

            let backend_endpoints = match self.shared.options.resolver.resolve().await {
                Ok(v) => v,
                Err(e) => {
                    eprintln!("Resolver failed: {}", e);
                    resolve_backoff.end_attempt(false);
                    continue;
                }
            };

            let backend_endpoints = backend_endpoints.into_iter().collect::<HashSet<_>>();

            let mut add_endpoints = Vec::new();
            {
                let mut state = self.shared.state.lock().await;

                let mut remove_endpoints = Vec::new();
                for existing_endpoint in state.backends.keys() {
                    if !backend_endpoints.contains(existing_endpoint) {
                        remove_endpoints.push(existing_endpoint.clone());
                    }
                }

                // TODO: Gracefully shut down all of these.
                for endpoint in remove_endpoints {
                    state.backends.remove(&endpoint);
                }

                for new_endpoint in &backend_endpoints {
                    if !state.backends.contains_key(new_endpoint) {
                        add_endpoints.push(new_endpoint.clone());
                    }
                }
            }

            let mut created_clients = vec![];
            for endpoint in add_endpoints {
                let client =
                    DirectClient::new(endpoint.clone(), self.shared.options.backend.clone());
                task::spawn(client.clone().run());
                created_clients.push((endpoint, client));
            }

            {
                let mut state = self.shared.state.lock().await;

                for (endpoint, client) in created_clients {
                    state.backends.insert(endpoint, client);
                }

                state.notify_all();
            }

            if let Err(_) = resolver_listener.recv().await {
                return;
            }
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
