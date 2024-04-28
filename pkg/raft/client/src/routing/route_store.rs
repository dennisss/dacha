use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;
use std::time::SystemTime;

use executor::sync::{AsyncVariable, AsyncVariableGuard, AsyncVariablePermit};

use crate::proto::*;

/// Amount of time after which we will consider a route to be stale and no
/// longer useable. (measured at the time at which the original server accounced
/// it).
const ROUTE_EXPIRATION_DURATION: Duration = Duration::from_secs(10);

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum RouteInitializerState {
    /// No processes are currently working to populate the route store with
    /// initial data.
    NoInitializers = 0,

    /// At least one process is currently working to get a complete set of
    /// routing information.
    ///
    /// The expectation is that the process(s) will soon (within a bounded
    /// amount of time) switch the state to Initialized (either due to a success
    /// or timeout/failure).
    Initializing = 1,

    /// At least one route discovery round has fully completed so we believe
    /// that the RouteStore contains a complete picture of the cluster.
    Initialized = 2,
}

/// Container of all server-to-server routing information known by the local
/// server.
#[derive(Clone)]
pub struct RouteStore {
    state: Arc<AsyncVariable<State>>,
}

struct State {
    /// TODO: When a connection times out we want to automatically remove it
    /// from this list.
    peers: HashMap<(GroupId, ServerId), PeerState>,
    local_route: Option<Route>,

    /// NOTE: These never change after the constructor.
    labels: Vec<RouteLabel>,

    initializers: RouteInitializerState,
}

struct PeerState {
    /// Route to this peer server.
    route: Route,

    /// Last time we received an acknowledgment that this peer server knows that
    /// the local server exists (at the current local_route).
    last_acknowledged_us: Option<SystemTime>,
}

impl RouteStore {
    pub fn new(labels: &[RouteLabel]) -> Self {
        Self {
            state: Arc::new(AsyncVariable::new(State {
                peers: HashMap::new(),
                local_route: None,
                labels: labels.to_vec(),
                initializers: RouteInitializerState::NoInitializers,
            })),
        }
    }

    pub async fn lock<'a>(&'a self) -> RouteStoreGuard<'a> {
        let mut state = self.state.lock().await.unwrap().enter();

        // All intermediate states / partial mutations to the route store should be ok
        unsafe { state.unpoison() };

        RouteStoreGuard { state }
    }
}

pub struct RouteStoreGuard<'a> {
    state: AsyncVariableGuard<'a, State>,
}

impl<'a> RouteStoreGuard<'a> {
    pub fn local_route(&self) -> Option<&Route> {
        self.state.local_route.as_ref()
    }

    pub fn set_local_route(&mut self, mut route: Route) {
        self.state
            .peers
            .remove(&(route.group_id(), route.server_id()));

        for label in self.state.labels.iter().cloned() {
            route.add_labels(label);
        }

        self.state.local_route = Some(route);

        for (_, peer) in &mut self.state.peers {
            peer.last_acknowledged_us = None;
        }

        self.state.notify_all();
    }

    pub fn set_initializer_state(&mut self, state: RouteInitializerState) {
        let new_value = core::cmp::max(self.state.initializers, state);

        if new_value != self.state.initializers {
            self.state.initializers = new_value;
            self.state.notify_all();
        }
    }

    pub fn initializer_state(&self) -> RouteInitializerState {
        self.state.initializers
    }

    fn should_select_route(&self, route: &Route) -> bool {
        for remote_label in route.labels() {
            if remote_label.optional() {
                continue;
            }

            let mut found = false;
            for local_label in &self.state.labels {
                if local_label.value() == remote_label.value() {
                    found = true;
                    break;
                }
            }

            if !found {
                return false;
            }
        }

        true
    }

    pub fn selected_routes(&self) -> impl Iterator<Item = &Route> {
        // let this = &*self;
        self.state
            .peers
            .values()
            .filter(move |r| self.should_select_route(&r.route))
            .map(|p| &p.route)
    }

    /// Looks up routing information for connecting to another server in the
    /// cluster by id. Also marks the request with routing metadata if a
    /// route is fond.
    pub fn lookup(&self, group_id: GroupId, server_id: ServerId) -> Option<&Route> {
        // TODO: Use the local route version if available.

        // TODO: Mark the route as recently used.

        self.state
            .peers
            .get(&(group_id, server_id))
            .filter(|r| self.should_select_route(&r.route))
            .map(|p| &p.route)
    }

    pub fn lookup_last_ack_time(
        &self,
        group_id: GroupId,
        server_id: ServerId,
    ) -> Option<Option<SystemTime>> {
        self.state
            .peers
            .get(&(group_id, server_id))
            .filter(|r| self.should_select_route(&r.route))
            .map(|p| p.last_acknowledged_us)
    }

    pub fn remote_groups(&self) -> HashSet<GroupId> {
        let mut groups = HashSet::new();
        for route in self.selected_routes() {
            groups.insert(route.group_id());
        }

        groups
    }

    pub fn remote_servers(&self, group_id: GroupId) -> HashSet<ServerId> {
        let mut servers = HashSet::new();
        for route in self.selected_routes() {
            if route.group_id() != group_id {
                continue;
            }

            servers.insert(route.server_id());
        }

        servers
    }

    pub fn serialize(&self) -> Announcement {
        let mut announcement = self.serialize_local_only();

        for peer in self.state.peers.values() {
            let mut r = peer.route.clone();
            r.set_is_local_route(false);
            announcement.add_routes(r);
        }

        announcement
    }

    pub fn serialize_local_only(&self) -> Announcement {
        let mut announcement = Announcement::default();
        let now = SystemTime::now();
        announcement.set_time(now);

        if let Some(local_route) = &self.state.local_route {
            let mut r = local_route.clone();
            r.set_last_seen(now);
            r.set_is_local_route(true);
            announcement.add_routes(r);
        }

        announcement
    }

    // TODO: Guard against using any times that are in the future.
    pub fn apply(&mut self, an: &Announcement) {
        let mut changed = false;

        // TODO: Also record the peer send time in the Announcement and use the min time
        // here.
        let now = SystemTime::now();
        let time_horizon = now - ROUTE_EXPIRATION_DURATION;

        let remote_time = SystemTime::from(an.time());
        let send_time = core::cmp::min(remote_time, now);

        // Identity of the server that created this announcement.
        let mut producer_id = None;

        let mut producer_knows_us = false;

        for new_route in an.routes().iter() {
            let new_route_key = (new_route.group_id(), new_route.server_id());

            if let Some(local_route) = &self.state.local_route {
                if (local_route.group_id(), local_route.server_id()) == new_route_key {
                    if local_route.target() == new_route.target() {
                        producer_knows_us = true;
                    }

                    continue;
                }
            }

            if new_route.is_local_route() {
                producer_id = Some((new_route.group_id(), new_route.server_id()));
            }

            // We will only accept the new path if it is fresher than the existing route
            // where freshness is defined by when the origin server broadcast this route.
            let should_insert = match self
                .state
                .peers
                .get(&(new_route.group_id(), new_route.server_id()))
                .map(|p| &p.route)
            {
                Some(old_route) => {
                    SystemTime::from(new_route.last_seen())
                        > SystemTime::from(old_route.last_seen())
                }
                None => true,
            };

            if should_insert && SystemTime::from(new_route.last_seen()) >= time_horizon {
                self.state.peers.insert(
                    (new_route.group_id(), new_route.server_id()),
                    PeerState {
                        route: new_route.as_ref().clone(),
                        last_acknowledged_us: None,
                    },
                );

                changed = true;
            }
        }

        if let Some(producer_key) = producer_id {
            if producer_knows_us {
                self.state
                    .peers
                    .get_mut(&producer_key)
                    .unwrap()
                    .last_acknowledged_us = Some(send_time);
                changed = true;
            }
        }

        self.state.peers.retain(|_, peer| {
            if SystemTime::from(peer.route.last_seen()) >= time_horizon {
                true
            } else {
                // println!("Route expired: {:?}", route);
                changed = true;
                false
            }
        });

        if changed {
            self.state.notify_all();
        }
    }

    pub async fn wait(self) {
        self.state.wait().await
    }
}
