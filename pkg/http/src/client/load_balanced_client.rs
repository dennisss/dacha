use std::collections::{HashMap, HashSet};
use std::net::SocketAddr;
use std::slice::SliceIndex;
use std::sync::Arc;

use common::async_std::channel;
use common::async_std::sync::Mutex;
use common::async_std::task;
use common::condvar::Condvar;
use common::errors::*;
use common::task::ChildTask;
use common::vec_hash_set::VecHashSet;
use crypto::random::RngExt;
use net::backoff::*;

use crate::client::client_interface::*;
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
    backends: VecHashSet<ResolvedEndpoint, Backend>,
}

struct Backend {
    client: DirectClient,
    task: ChildTask,
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
            match resolve_backoff.start_attempt() {
                ExponentialBackoffResult::Start => {}
                ExponentialBackoffResult::StartAfter(wait_time) => {
                    task::sleep(wait_time).await;
                }
                ExponentialBackoffResult::Stop => {
                    eprintln!("LoadBalancedClient failed too many times.");
                    return;
                }
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
                    // println!("Remove client: {:?}", endpoint);
                    state.backends.remove(&endpoint);
                }

                for new_endpoint in &backend_endpoints {
                    if !state.backends.contains_key(new_endpoint) {
                        add_endpoints.push(new_endpoint.clone());
                    }
                }
            }

            let mut created_backends = vec![];
            for endpoint in add_endpoints {
                // println!("Create client: {:?}", endpoint);
                let client =
                    DirectClient::new(endpoint.clone(), self.shared.options.backend.clone());
                let task = ChildTask::spawn(client.clone().run());

                created_backends.push((endpoint, Backend { client, task }));
            }

            {
                let mut state = self.shared.state.lock().await;

                for (endpoint, backend) in created_backends {
                    state.backends.insert(endpoint, backend);
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
    async fn request(
        &self,
        request: Request,
        request_context: ClientRequestContext,
    ) -> Result<Response> {
        let client;
        loop {
            let state = self.shared.state.lock().await;

            // TODO: Distinguish between the backends list being empty and the resolver
            // still pending an initial response or being in an error state.
            if state.backends.values().is_empty() {
                state.wait(()).await;
                continue;
            }

            // TODO: Increment the index if we encounter a failing client.
            let mut rng = crypto::random::clocked_rng();

            if request_context.wait_for_ready {
                client = rng.choose(state.backends.values()).client.clone();
            } else {
                // TODO: We should use the healthy subset for wait_for_ready as well but we need
                // to ensure that we limit the max enqueued requests per backend.
                let mut healthy_subset = vec![];
                for backend in state.backends.values() {
                    if backend.client.current_state().await != ClientState::Failure {
                        healthy_subset.push(&backend.client);
                    }
                }

                if healthy_subset.is_empty() {
                    return Err(crate::v2::ProtocolErrorV2 {
                        code: crate::v2::ErrorCode::STREAM_CLOSED,
                        message: "All backends currently failing".into(),
                        local: false,
                    }
                    .into());
                }

                client = (*rng.choose(&healthy_subset)).clone();
            }

            break;
        }

        client.request(request, request_context).await
    }

    async fn current_state(&self) -> ClientState {
        // TODO: Implement me
        ClientState::NotConnected
    }
}
