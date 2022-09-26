use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::net::SocketAddr;
use std::slice::SliceIndex;
use std::sync::{Arc, Weak};

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
    backends: HashMap<usize, Backend>,
    last_backend_id: usize,
    failing: bool,
}

struct Backend {
    endpoint: ResolvedEndpoint,
    client: DirectClient,
    task: ChildTask,
    shutting_down: bool,
}

impl LoadBalancedClient {
    pub fn new(options: LoadBalancedClientOptions) -> Self {
        Self {
            shared: Arc::new(Shared {
                options,
                state: Condvar::new(State {
                    backends: HashMap::new(),
                    last_backend_id: 0,
                    failing: false,
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

            // TODO: Limit the max frequency of this returning.
            let resolved_result = self.shared.options.resolver.resolve().await;

            let mut state = self.shared.state.lock().await;

            let backend_endpoints = match resolved_result {
                Ok(v) => {
                    state.failing = v.is_empty();
                    v
                }
                Err(e) => {
                    // Set as failing.
                    eprintln!("Resolver failed: {}", e);
                    resolve_backoff.end_attempt(false);
                    state.failing = true;
                    state.notify_all();
                    continue;
                }
            };

            // This is the set of all backends we want to connect to.
            let mut add_endpoints = backend_endpoints.into_iter().collect::<HashSet<_>>();

            // TODO: Limit the max size of state.backends at any given point in time.
            for (id, existing_backend) in state.backends.iter_mut() {
                if existing_backend.shutting_down {
                    continue;
                }

                if add_endpoints.contains(&existing_backend.endpoint) {
                    // Already exists
                    add_endpoints.remove(&existing_backend.endpoint);
                } else {
                    // TODO: Allow keeping these running until some of the new endpoints become
                    // healthy.
                    existing_backend.shutting_down = true;
                    existing_backend.client.shutdown().await;
                }
            }

            for endpoint in add_endpoints {
                println!("[http::Client] Start new backend client: {:?}", endpoint);

                let id = state.last_backend_id + 1;
                state.last_backend_id = id;

                // TODO:Also need to add support in DirectClient for calling it.
                let client = DirectClient::new(
                    endpoint.clone(),
                    self.shared.options.backend.clone(),
                    self.shared.clone(),
                );
                // TODO: Also must use a client_runner to delete the backend eventually.
                let task = ChildTask::spawn(Self::run_backend_client(
                    Arc::downgrade(&self.shared),
                    id,
                    client.run(),
                ));

                state.backends.insert(
                    id,
                    Backend {
                        endpoint,
                        client,
                        task,
                        shutting_down: false,
                    },
                );
            }

            state.notify_all();
            drop(state);

            if let Err(_) = resolver_listener.recv().await {
                return;
            }
        }
    }

    async fn run_backend_client<F: Future<Output = ()> + Send>(
        shared: Weak<Shared>,
        backend_id: usize,
        f: F,
    ) {
        f.await;
        if let Some(shared) = shared.upgrade() {
            let mut state = shared.state.lock().await;
            if let Some(backend) = state.backends.remove(&backend_id) {
                if !backend.shutting_down {
                    state.failing = true;
                    eprintln!("DirectClient fails before shut down");
                }
            }

            state.notify_all();
        }
    }

    // async fn run_connection(shared: Weak<Shared>)
}

#[async_trait]
impl ClientEventListener for Shared {
    async fn handle_client_state_change(&self) {
        self.state.lock().await.notify_all();
    }
}

#[async_trait]
impl ClientInterface for LoadBalancedClient {
    async fn request(
        &self,
        request: Request,
        request_context: ClientRequestContext,
    ) -> Result<Response> {
        // TODO: If a backend becomes healthy, we won't want to rush all enqueued
        // requests to start using it as it may only be able to handle one more request.

        // TODO: Should we be concerned about too many requests queuing up at this
        // stage?

        let client;
        loop {
            let state = self.shared.state.lock().await;

            // Fail if the resolver failed or the resolver succeeded and no backends were
            // found.
            if state.failing && !request_context.wait_for_ready {
                return Err(crate::v2::ProtocolErrorV2 {
                    code: crate::v2::ErrorCode::REFUSED_STREAM,
                    message: "Failed to resolve any remote backends".into(),
                    local: true,
                }
                .into());
            }

            // Still waiting for at least one pass of the resolver to finish.
            if state.backends.is_empty() {
                state.wait(()).await;
                continue;
            }

            let mut healthy_backends = vec![];
            for (id, backend) in &state.backends {
                if !backend.shutting_down
                    && backend.client.current_state().await != ClientState::Failure
                {
                    healthy_backends.push(backend);
                }
            }

            if healthy_backends.is_empty() {
                if !request_context.wait_for_ready {
                    return Err(crate::v2::ProtocolErrorV2 {
                        code: crate::v2::ErrorCode::REFUSED_STREAM,
                        message: "All backends currently failing".into(),
                        local: false,
                    }
                    .into());
                }

                continue;
            }

            // TODO: Record which endpoint we are using so that future retries are able to
            // explicitly retry on a distinct backend.
            let mut rng = crypto::random::clocked_rng();
            client = rng.choose(&healthy_backends).client.clone();

            break;
        }

        // TODO: Use a 'enqueue_request' interface so that we can
        client.request(request, request_context).await
    }

    async fn current_state(&self) -> ClientState {
        // TODO: Implement me
        ClientState::Idle
    }
}
