use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::net::SocketAddr;
use std::slice::SliceIndex;
use std::sync::{Arc, Weak};
use std::time::{Duration, Instant};

use common::errors::*;
use common::hash::{FastHasherBuilder, SumHasherBuilder};
use common::vec_hash_set::VecHashSet;
use crypto::hasher::Hasher;
use crypto::random::{RngExt, SharedRngExt};
use executor::cancellation::CancellationToken;
use executor::channel;
use executor::child_task::ChildTask;
use executor::lock;
use executor::loop_throttler::LoopThrottler;
use executor::sync::AsyncVariable;
use net::backoff::*;

use crate::client::client_interface::*;
use crate::client::direct_client::DirectClient;
use crate::client::direct_client::DirectClientOptions;
use crate::client::resolver::{ResolvedEndpoint, Resolver};
use crate::request::Request;
use crate::response::Response;

use super::direct_client::ClientHeartbeatOptions;

/*
Backend Selection / Subsetting Algorithm
Parameters:
    - subset_size: Target number of servers to be connected to.
    - max_backend_count: Maximum number of backends (separate IPs we can be connected to).
        - '> subset_size'
    - healthy_backend_threshold:
        - '<= subset_size'

1. Get a full list of backends from the resolver
2. Sort this list by a hash of each endpoint.
    - Each hash is seeded by a random per-client id
3. Set N = subset_size
4. While N < max_backend_count
    - Let K = # of already failing connections in the backends from #3
    - if N - K < subset_size, N += 1
5. Pull out the first N backends in the list from #3
6. Create backend clients from any the N selected backends missing a client
    - (while the current number of client instances is < max_backend_count)
7. Shutdown from any backend clients not in the N backends list
    - Clients in a failing state can be immediately shut down
    - Clients in other states can be shutdown while the # of non-failing backends is >= healthy_backend_threshold.

TODO: Before shutting down connections with session affinity, wait 5 seconds before shutting it down (during this time, don't assign new sessions to the backend and try to re-assign requests to existing connectiosn tht )

This algorithm has the property that backends are uniformly distributed across backends while minimizing churn if backends are added/removed from the available list. Additionally it is resilient to individual backends failing by bleeding out of the target subset temporarily with the nice properly that we will re-balance once failures are resolved.

We will retry this whenever:
- A backend client's state changes.
- The resolver has a new set of endpoints.

NOTE: A big assumption with the LoadBalancedClient is that all endpoints are equivalent in terms of proximity. You'll need more than this if you want to load balance across many regions.
*/

#[derive(Clone)]
pub struct LoadBalancedClientOptions {
    pub resolver_backoff: ExponentialBackoffOptions,

    /// Target number of distinct backends to have connected.
    pub subset_size: usize,

    /// If after subsetting we don't have at least this many client instances,
    /// we will create redundant connections to endpoints in our subset to reach
    /// this backend count.
    ///
    /// This is mainly to be used for HTTP2 only clients to improve network
    /// utilization (as each HTTP2 connection)
    ///
    /// TODO: Deduplicate this with the mechanism in DirectClient for making
    /// many connection instances of an HTTP1 connection.
    pub target_parallelism: usize,

    /// Maximum number of DirectClient instances to have open at any given time.
    pub max_backend_count: usize,

    pub healthy_backend_threshold: f32,

    // TODO: need a policy for how to pick a backend.
    pub backend: DirectClientOptions,
}

impl Default for LoadBalancedClientOptions {
    fn default() -> Self {
        LoadBalancedClientOptions {
            backend: DirectClientOptions {
                tls: None,
                force_http2: false,
                upgrade_plaintext_http2: false,
                connection_backoff: ExponentialBackoffOptions {
                    base_duration: Duration::from_millis(100),
                    jitter_duration: Duration::from_millis(200),
                    max_duration: Duration::from_secs(20),
                    cooldown_duration: Duration::from_secs(60),
                    max_num_attempts: 0,
                },
                connect_timeout: Duration::from_millis(2000),
                idle_timeout: Duration::from_secs(5 * 60), // 5 minutes.
                /// MUST be <= v2::ConnectionOptions::max_enqueued_requests
                max_outstanding_requests: 100,
                max_num_connections: 10,
                http1_max_requests_per_connection: 1,
                remote_shutdown_is_failure: false,
                eagerly_connect: true,
                heartbeat: ClientHeartbeatOptions {
                    // Note that this must be at least 5 minutes to be compatible with the gRPC
                    // server side min interval of 5 minutes.
                    ping_interval: Duration::from_secs(20 * 60),
                    ping_timeout: Duration::from_secs(10),
                },
            },
            resolver_backoff: ExponentialBackoffOptions {
                base_duration: Duration::from_millis(100),
                jitter_duration: Duration::from_millis(200),
                max_duration: Duration::from_secs(20),
                cooldown_duration: Duration::from_secs(60),
                max_num_attempts: 0,
            },
            subset_size: 10,
            max_backend_count: 14,
            healthy_backend_threshold: 0.8,
            target_parallelism: 0,
        }
    }
}

impl LoadBalancedClientOptions {
    pub fn default_for_dns() -> Self {
        let mut options = Self::default();

        // Normally DNS exposes load balancer IPs instead of raw server ips so not worth
        // having a lot of connections.
        options.subset_size = 1;
        options.max_backend_count = 10;

        options
    }
}

#[derive(Clone)]
pub struct LoadBalancedClient {
    shared: Arc<Shared>,
}

struct Shared {
    client_id: u64,
    resolver: Arc<dyn Resolver>,

    options: LoadBalancedClientOptions,
    state: AsyncVariable<State>,

    /// Event queue used to notify the main worker thread other
    /// LoadBalancedClient to handle.
    ///
    /// TODO: Make this into a bit map.
    event_sender: channel::Sender<Event>,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum Event {
    BackendStateChange,
    ResolverChange,
    Cancelled,
}

struct State {
    /// Set of backends to which we connected to (indexed by monotonic id).
    backends: HashMap<usize, Backend, FastHasherBuilder>,

    last_backend_id: usize,

    /// If true, the client is actively shutting down (due to the cancellation
    /// token).
    shutting_down: bool,

    /// Current state of the backend resolver.
    /// - Idle: Meaning we haven't yet resolved any backends (clean client)
    /// - Failed: Latest resolving attempt has failed.
    /// - Ready: Latest resolving attempt has succeeded.
    resolver_state: ClientState,

    /// Immediately taken by the run() thread.
    event_receiver: Option<channel::Receiver<Event>>,
}

#[derive(Clone, PartialEq, Eq)]
struct BackendKey {
    /// Hash of the rest of this struct (endpoint, index) which is keyed by the
    /// current client_id.
    hash: u64,

    endpoint: ResolvedEndpoint,

    /// Out of all the backends with the same 'endpoint', this is the index of
    /// this one.
    index: usize,
}

impl std::hash::Hash for BackendKey {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        state.write_u64(self.hash);
    }
}

struct Backend {
    key: BackendKey,

    client: DirectClient,

    // TODO: Make sure this doesn't get cancelled but instead we wait for it to finish.
    task: ChildTask,

    /// If true, then we've initiated a client-side shutdown of this backend
    /// because its no longer recommended by the resolver.
    shutting_down: bool,
}

impl Backend {
    async fn is_healthy(&self) -> bool {
        if self.shutting_down {
            return false;
        }

        let state = self.client.current_state().await;
        state != ClientState::Failure
    }
}

impl LoadBalancedClient {
    pub fn new(
        client_id: u64,
        resolver: Arc<dyn Resolver>,
        options: LoadBalancedClientOptions,
    ) -> Self {
        let (event_sender, event_receiver) = channel::unbounded();

        Self {
            shared: Arc::new(Shared {
                client_id,
                resolver,
                options,
                state: AsyncVariable::new(State {
                    backends: HashMap::with_hasher(FastHasherBuilder::default()),
                    last_backend_id: 0,
                    shutting_down: false,
                    resolver_state: ClientState::Idle,
                    event_receiver: Some(event_receiver),
                }),
                event_sender,
            }),
        }
    }

    // NOT CANCEL SAFE
    pub async fn run(self, cancellation_token: Arc<dyn CancellationToken>) -> Result<()> {
        let event_receiver = lock!(state <= self.shared.state.lock().await.unwrap(), {
            state.event_receiver.take().unwrap()
        });

        let mut resolve_backoff =
            ExponentialBackoff::new(self.shared.options.resolver_backoff.clone());

        // Register events for when the notifier
        {
            let sender = self.shared.event_sender.clone();
            self.shared
                .resolver
                .add_change_listener(Box::new(move || {
                    match sender.try_send(Event::ResolverChange) {
                        Ok(()) => true,
                        Err(channel::TrySendError::Full(_)) => true,
                        Err(channel::TrySendError::Closed(_)) => false,
                    }
                }))
                .await;
        }

        let mut latest_resolved_endpoints = None;

        let mut throttler = LoopThrottler::new(10, Duration::from_secs(2));

        while throttler
            .start_cancellable_iteration(cancellation_token.as_ref())
            .await
        {
            let mut received_events = HashSet::new();

            // Always retry resolving if we don't have data yet.
            if !latest_resolved_endpoints.is_some() {
                received_events.insert(Event::ResolverChange);
            }

            // Wait for something to happen.
            loop {
                // TODO: This must race for something to happen.
                let e = {
                    if received_events.is_empty() {
                        let f = executor::future::race(
                            async { event_receiver.recv().await },
                            executor::future::map(
                                cancellation_token.wait_for_cancellation(),
                                |()| Ok(Event::Cancelled),
                            ),
                        );

                        match f.await {
                            Ok(e) => e,
                            // Should never happen since we hold a reference to the other side.
                            Err(_) => panic!(),
                        }
                    } else {
                        match event_receiver.try_recv() {
                            Ok(v) => v,
                            Err(_) => break,
                        }
                    }
                };

                received_events.insert(e);
            }

            if received_events.contains(&Event::Cancelled) {
                break;
            }

            let mut resolved_result = None;
            if received_events.contains(&Event::ResolverChange) {
                match resolve_backoff.start_attempt() {
                    ExponentialBackoffResult::Start => {}
                    ExponentialBackoffResult::StartAfter(wait_time) => {
                        // TODO: Account for cancellation.
                        executor::sleep(wait_time).await.unwrap();
                    }
                    ExponentialBackoffResult::Stop => {
                        eprintln!("LoadBalancedClient failed too many times.");
                        panic!();
                    }
                }

                // TODO: Have a timeout on how long this is allowed to run for.
                // If it takes too long, then we should mark as failed.
                let res = self.shared.resolver.resolve().await;

                resolve_backoff.end_attempt(res.is_ok());

                resolved_result = Some(res);
            }

            // TODO: Verify everything after this is syncronous to ensure this whole
            // function is cancel safe.
            let mut state = self.shared.state.lock().await.unwrap().enter();

            if let Some(resolved_result) = resolved_result {
                match resolved_result {
                    Ok(v) => {
                        // NOTE: We must not unlock the state until after the backend client list
                        // has been reconciled otherwise the client may see
                        // a ready resolver with no backends attached.
                        state.resolver_state = ClientState::Ready;
                        latest_resolved_endpoints = Some(v)
                    }
                    Err(e) => {
                        // Set as failing.
                        eprintln!("Resolver failed: {}", e);
                        resolve_backoff.end_attempt(false);
                        state.resolver_state = ClientState::Failure;
                        state.notify_all();

                        // Retry concacting the resolver. Old state is invalidated.
                        latest_resolved_endpoints = None;
                        continue;
                    }
                }
            }

            let backend_endpoints = latest_resolved_endpoints.as_ref().unwrap();

            let mut available_endpoints = vec![];

            for ep in backend_endpoints.iter() {
                let index = 0;
                let mut hasher = crypto::sip::SipHasher::default_rounds_with_key_halves(
                    self.shared.client_id,
                    index as u64,
                );
                std::hash::Hash::hash(ep, &mut hasher);

                let hash = hasher.finish_u64();

                available_endpoints.push(BackendKey {
                    hash,
                    endpoint: ep.clone(),
                    index,
                });
            }

            // NOTE: May be incorrect if we have duplicate hash keys.
            available_endpoints.sort_by_key(|v| v.hash);

            available_endpoints.dedup();

            // NOTE: If there are mutiple client instances pointed at a single endpoint, if
            // any of them are healthy, we consider the whole endpoint to be healthy.
            let mut current_endpoint_health = HashMap::new();
            current_endpoint_health.reserve(state.backends.len());

            let mut currently_healthy_backends = HashSet::new();
            currently_healthy_backends.reserve(state.backends.len());

            let mut num_healthy_current_backends = 0;
            for (_, backend) in state.backends.iter() {
                let healthy = backend.is_healthy().await;

                *current_endpoint_health
                    .entry(&backend.key.endpoint)
                    .or_default() |= healthy;

                if healthy {
                    currently_healthy_backends.insert(backend.key.clone());
                }
            }

            // Select our target client subset.
            let mut target_subset = {
                // Number of backends (starting at the front of the list) we have picked.
                let mut n = 0;

                // Number of plausibly healthy backends from the list we have picked so far.
                let mut num_healthy = 0;

                while n < available_endpoints.len()
                    && n < self.shared.options.max_backend_count
                    && num_healthy < self.shared.options.subset_size
                {
                    let endpoint_key = &available_endpoints[n];

                    let maybe_healthy = current_endpoint_health
                        .get(&endpoint_key.endpoint)
                        .cloned()
                        .unwrap_or(true);

                    if maybe_healthy {
                        num_healthy += 1;
                    }

                    n += 1;
                }

                available_endpoints.truncate(n);
                available_endpoints
            };

            // Tile the subset to satisfy parallelism targets.
            if !target_subset.is_empty() {
                let mut i = 0;
                while target_subset.len() < self.shared.options.target_parallelism
                    && target_subset.len() < self.shared.options.max_backend_count
                {
                    let mut key = target_subset[i].clone();
                    key.index += 1;

                    let mut hasher = crypto::sip::SipHasher::default_rounds_with_key_halves(
                        self.shared.client_id,
                        key.index as u64,
                    );
                    std::hash::Hash::hash(&key.endpoint, &mut hasher);

                    key.hash = hasher.finish_u64();
                    target_subset.push(key);

                    i += 1;
                }
            }

            let mut target_subset = {
                let mut out = HashSet::with_hasher(SumHasherBuilder::default());
                out.extend(target_subset.into_iter());
                out
            };

            let min_healthy_count = core::cmp::max(
                ((target_subset.len() as f32) * self.shared.options.healthy_backend_threshold)
                    as usize,
                1,
            );

            for (id, existing_backend) in state.backends.iter_mut() {
                if existing_backend.shutting_down {
                    continue;
                }

                if target_subset.contains(&existing_backend.key) {
                    // Already exists
                    target_subset.remove(&existing_backend.key);
                } else {
                    if currently_healthy_backends.contains(&existing_backend.key) {
                        if currently_healthy_backends.len() <= min_healthy_count {
                            continue;
                        }

                        currently_healthy_backends.remove(&existing_backend.key);
                    }

                    existing_backend.shutting_down = true;
                    existing_backend.client.shutdown().await;
                }
            }

            for endpoint_key in target_subset {
                if state.backends.len() >= self.shared.options.max_backend_count {
                    break;
                }

                // println!(
                //     "[http::Client] Start new backend client: {} (#{})",
                //     endpoint_key.endpoint, endpoint_key.index
                // );

                let id = state.last_backend_id + 1;
                state.last_backend_id = id;

                // TODO:Also need to add support in DirectClient for calling it.
                let client = DirectClient::new(
                    endpoint_key.endpoint.clone(),
                    self.shared.options.backend.clone(),
                    // TODO: Make this into a weak pointer.
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
                        key: endpoint_key,
                        client,
                        task,
                        shutting_down: false,
                    },
                );
            }

            state.notify_all();
            state.exit();
        }

        // Shutdown.
        self.perform_shutdown().await?;

        Ok(())
    }

    async fn perform_shutdown(self) -> Result<()> {
        let mut backends = lock!(state <= self.shared.state.lock().await?, {
            state.shutting_down = true;
            state.notify_all();

            state.backends.drain().collect::<Vec<_>>()
        });

        for (_, backend) in &mut backends {
            if backend.shutting_down {
                continue;
            }

            backend.shutting_down = true;
            backend.client.shutdown().await;
        }

        for (_, backend) in backends {
            backend.task.join().await;
        }

        Ok(())
    }

    async fn run_backend_client<F: Future<Output = ()> + Send>(
        shared: Weak<Shared>,
        backend_id: usize,
        f: F,
    ) {
        f.await;

        // Deleting the backend entry after it is done running.

        let shared = match shared.upgrade() {
            Some(v) => v,
            None => return,
        };

        let mut state = shared.state.lock().await.unwrap().enter();

        // NOTE: If we are shutting down the entire client, then perform_shutdown()
        // would have already removed all these entries.
        if state.backends.remove(&backend_id).is_some() {
            state.notify_all();
        }

        state.exit();
    }
}

impl ClientEventListener for Shared {
    fn handle_client_state_change(&self) {
        // NOTE: The state watchers will be indirectly updated by the main thread but we
        // don't do it here to avoid a possibly nested lock of the state.
        let _ = self.event_sender.try_send(Event::BackendStateChange);
    }
}

fn number_distance(a: u64, b: u64) -> u64 {
    let normal_distance = {
        if a >= b {
            a - b
        } else {
            b - a
        }
    };

    let wrap_distance = {
        if a >= b {
            (u64::MAX - a) + b
        } else {
            (u64::MAX - b) + a
        }
    };

    core::cmp::min(normal_distance, wrap_distance)
}

#[async_trait]
impl ClientInterface for LoadBalancedClient {
    async fn request(
        &self,
        request: Request,
        request_context: ClientRequestContext,
        response_context: &mut ClientResponseContext,
    ) -> Result<Response> {
        // TODO: If a backend becomes healthy, we won't want to rush all enqueued
        // requests to start using it as it may only be able to handle one more request.
        // ^ This is partially mitigated by the DirectClient in-flight request limits.

        // TODO: Should we be concerned about too many requests queuing up at this
        // stage?
        // ^ Yes, limit the max queue length here.

        let client;
        loop {
            let state = self.shared.state.lock().await?.read_exclusive();

            if state.shutting_down {
                return Err(crate::v2::ProtocolErrorV2 {
                    code: crate::v2::ErrorCode::REFUSED_STREAM,
                    message: "Client is shutting down".into(),
                    local: true,
                }
                .into());
            }

            if state.backends.is_empty() {
                if state.resolver_state == ClientState::Idle || request_context.wait_for_ready {
                    // Ok to wait as we haven't finished one attempt for the
                    // resolver yet.
                } else if state.resolver_state == ClientState::Failure {
                    return Err(crate::v2::ProtocolErrorV2 {
                        code: crate::v2::ErrorCode::REFUSED_STREAM,
                        message: "Failed to resolve any remote backends".into(),
                        local: true,
                    }
                    .into());
                } else if state.resolver_state == ClientState::Ready {
                    return Err(crate::v2::ProtocolErrorV2 {
                        code: crate::v2::ErrorCode::REFUSED_STREAM,
                        message: "No remote backends available to serve the request.".into(),
                        local: true,
                    }
                    .into());
                }

                state.wait().await;
                continue;
            }

            let selection_key = {
                if let Some(affinity) = &request_context.affinity {
                    if let Some(cache) = &affinity.cache {
                        if let Some(backend_id) = cache.get(affinity.key) {
                            if let Some(backend) = state.backends.get(&(backend_id as usize)) {
                                if !backend.shutting_down
                                    && backend.client.current_state().await != ClientState::Failure
                                {
                                    client = backend.client.clone();
                                    break;
                                }
                            }
                        }
                    }

                    // Protect against easy to predict affinity keys.
                    let mut hasher = crypto::sip::SipHasher::default_rounds_with_key_halves(
                        self.shared.client_id,
                        0,
                    );
                    hasher.update(&affinity.key.hash().to_ne_bytes());
                    hasher.finish_u64()
                } else {
                    crypto::random::clocked_rng().uniform::<u64>()
                }
            };

            // Pick a backend.
            // We want to prioritize already Ready ones to avoid newly added connections
            // slowing things down.

            let mut best_ready_backend_id = None;
            let mut best_ready_distance = u64::MAX;

            let mut best_any_backend_id = None;
            let mut best_any_distance = u64::MAX;

            for (backend_id, backend) in state.backends.iter() {
                if backend.shutting_down {
                    continue;
                }

                let state = backend.client.current_state().await;
                if state == ClientState::Failure {
                    continue;
                }

                if !request_context.affinity.is_some() && state == ClientState::Congested {
                    continue;
                }

                let distance = number_distance(selection_key, backend.key.hash);

                if state == ClientState::Ready && distance < best_ready_distance {
                    best_ready_distance = distance;
                    best_ready_backend_id = Some(*backend_id);
                }

                if distance < best_any_distance {
                    best_any_distance = distance;
                    best_any_backend_id = Some(*backend_id);
                }
            }

            let best_backend_id = best_ready_backend_id.or(best_any_backend_id);

            if best_backend_id.is_none() {
                if !request_context.wait_for_ready {
                    return Err(crate::v2::ProtocolErrorV2 {
                        code: crate::v2::ErrorCode::REFUSED_STREAM,
                        message: "All backends currently failing or congested".into(),
                        local: false,
                    }
                    .into());
                }

                state.wait().await;
                continue;
            }

            if let Some(affinity) = &request_context.affinity {
                if let Some(cache) = &affinity.cache {
                    cache.set(affinity.key, best_backend_id.unwrap() as u64);
                }
            }

            // TODO: Record which endpoint we are using so that future retries are able to
            // explicitly retry on a distinct backend.
            let backend = state.backends.get(&best_backend_id.unwrap()).unwrap();

            response_context.selected_endpoint = Some(backend.key.endpoint.clone());

            client = backend.client.clone();

            break;
        }

        // TODO: Use a 'enqueue_request' interface so that we can
        client
            .request(request, request_context, response_context)
            .await
    }

    async fn current_state(&self) -> ClientState {
        // Readiness is when we have >50% of backends as 'Ready'.

        // TODO: Implement me
        ClientState::Idle
    }
}
