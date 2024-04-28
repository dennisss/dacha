use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use base_error::*;
use executor::bundle::TaskBundle;

use crate::proto::*;
use crate::routing::route_store::*;

/// Time between announcement requests sent directly to each peer server.
const SERVER_POLL_RATE: Duration = Duration::from_millis(2000);

/// Interval at which to re-check if any servers should be re-polled.
const CYCLE_PERIOD: Duration = Duration::from_millis(1000);

#[derive(Default, Clone)]
pub struct DiscoveryClientOptions {
    pub seeds: Vec<String>,

    /// If true, we will send non-empty advertisements to all seed servers and
    /// additionally we will periodically poll all known servers in the
    /// RouteStore.
    pub active_broadcaster: bool,
}

/// Most basic mode of discover service based on an initial list of server
/// addresses. We assume that each server listed equally represents the entire
/// cluster
///
/// We make no assumptions about the ids or memberships of any of the servers in
/// the list and they can be dns names or load balanced end points if convenient
///
/// The general strategy that this uses is as follows:
/// - For new servers, we immediately ask the seed servers for an initial
///   cluster configuration
/// - Starting once we are started up, every server will perform a low frequency
///   sync with the seed servers
/// - Separately we'd like to use a higher frequency heartbeat style
///   decentralized gossip protocol between all other nodes in the cluster
///   (using this layer allows for sharing of configurations even in the
///   presense of failed seed servers)
pub struct DiscoveryClient {
    route_store: RouteStore,
    options: DiscoveryClientOptions,
}

// TODO: Consider holding onto the list of seed servers in the long term (less
// periodically refresh our list with them) In this way, we may not even need a
// gossip protocol if we assume that we have a set of

impl DiscoveryClient {
    pub async fn create(route_store: RouteStore, options: DiscoveryClientOptions) -> Self {
        if !options.seeds.is_empty() {
            route_store
                .lock()
                .await
                .set_initializer_state(RouteInitializerState::Initializing);
        }

        Self {
            route_store,
            options,
        }
    }

    pub async fn run(self) -> Result<()> {
        #[derive(Clone)]
        struct AddrState {
            // Last time we tried sending a requiest to each remote server.
            last_send_attempt: SystemTime,

            channel: Arc<rpc::Http2Channel>,
        }

        let mut states: HashMap<String, AddrState> = HashMap::new();
        let mut initialized = false;

        loop {
            // Select addresses to poll.
            let (addrs, request) = {
                let now = SystemTime::now();

                let route_store = self.route_store.lock().await;

                // Next value of 'states' constructed without any garbage collected  routes.
                let mut new_states = HashMap::new();

                let mut selected_addrs = vec![];

                let mut maybe_select_addr = |addr: String| {
                    // Rate limit individual attempts per address
                    if let Some(state) = states.get(&addr) {
                        new_states.insert(addr.clone(), state.clone());

                        if state.last_send_attempt + SERVER_POLL_RATE > now {
                            return;
                        }
                    }

                    selected_addrs.push(addr);
                };

                for addr in &self.options.seeds {
                    maybe_select_addr(addr.clone());
                }

                if self.options.active_broadcaster {
                    // TODO: Make sure this doesn't get marked as a usage of the route.
                    for route in route_store.selected_routes() {
                        let addr = format!("http://{}", route.target().addr());
                        maybe_select_addr(addr);
                    }
                }

                states = new_states;

                if selected_addrs.is_empty() {
                    executor::timeout(CYCLE_PERIOD, async {
                        route_store.wait().await;
                        // Do some rough batching since servers will tend to send requests to us all
                        // at once when they get a multicast from us.
                        executor::sleep(Duration::from_millis(10)).await;
                    })
                    .await;

                    continue;
                }

                let mut announcement = Announcement::default();
                if self.options.active_broadcaster {
                    announcement = route_store.serialize();
                }

                (selected_addrs, announcement)
            };

            // Send to all addresses.
            {
                let mut reqs = vec![];

                let this = &self;
                let request_ref = &request;

                let now = SystemTime::now();
                for addr in &addrs {
                    let channel = match states.get(addr) {
                        Some(state) => state.channel.clone(),
                        None => {
                            let channel = Arc::new(
                                rpc::Http2Channel::create(http::ClientOptions::from_uri(
                                    &addr.parse()?,
                                )?)
                                .await?,
                            );

                            states.insert(
                                addr.clone(),
                                AddrState {
                                    last_send_attempt: now,
                                    channel: channel.clone(),
                                },
                            );

                            channel
                        }
                    };

                    reqs.push(async move {
                        let _ = executor::timeout(
                            Duration::from_millis(1000), // < Servers may frequently be offline
                            this.call_single_server(channel, request_ref),
                        )
                        .await;
                    });
                }

                common::futures::future::join_all(reqs).await;
            }

            // Set the attempt time to a time after the attempt has completed.
            let now = SystemTime::now();
            for addr in addrs {
                states.get_mut(&addr).unwrap().last_send_attempt = now;
            }

            if !initialized {
                initialized = true;
                if !self.options.seeds.is_empty() {
                    self.route_store
                        .lock()
                        .await
                        .set_initializer_state(RouteInitializerState::Initialized);
                }
            }

            // TODO: Also need backoff for addresses that are failing
            // (especially for seed list addresses which may fail forever if the
            // server was not started with the expectation of functionating
            // always)
        }

        Ok(())
    }

    async fn call_single_server(
        &self,
        channel: Arc<rpc::Http2Channel>,
        request: &Announcement,
    ) -> Result<()> {
        let stub = DiscoveryStub::new(channel);
        let res = stub
            .Announce(&rpc::ClientRequestContext::default(), request)
            .await
            .result?;

        let mut store = self.route_store.lock().await;
        store.apply(&res);

        Ok(())
    }
}
