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

/// Container of all server-to-server routing information known by the local
/// server.
#[derive(Clone)]
pub struct RouteStore {
    state: Arc<AsyncVariable<State>>,
}

struct State {
    /// TODO: When a connection times out we want to automatically remove it
    /// from this list.
    routes: HashMap<(GroupId, ServerId), Route>,
    local_route: Option<Route>,

    /// NOTE: These never change after the constructor.
    labels: Vec<RouteLabel>,
}

impl RouteStore {
    pub fn new(labels: &[RouteLabel]) -> Self {
        Self {
            state: Arc::new(AsyncVariable::new(State {
                routes: HashMap::new(),
                local_route: None,
                labels: labels.to_vec(),
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
    pub fn set_local_route(&mut self, mut route: Route) {
        self.state
            .routes
            .remove(&(route.group_id(), route.server_id()));

        for label in self.state.labels.iter().cloned() {
            route.add_labels(label);
        }

        self.state.local_route = Some(route);
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

    fn selected_routes(&self) -> impl Iterator<Item = &Route> {
        // let this = &*self;
        self.state
            .routes
            .values()
            .filter(move |r| self.should_select_route(*r))
    }

    /// Looks up routing information for connecting to another server in the
    /// cluster by id. Also marks the request with routing metadata if a
    /// route is fond.
    pub fn lookup(&self, group_id: GroupId, server_id: ServerId) -> Option<&Route> {
        // TODO: Use the local route version if available.

        // TODO: Mark the route as recently used.

        self.state
            .routes
            .get(&(group_id, server_id))
            .filter(|r| self.should_select_route(*r))
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

        for route in self.state.routes.values() {
            announcement.add_routes(route.clone());
        }

        announcement
    }

    pub fn serialize_local_only(&self) -> Announcement {
        let mut announcement = Announcement::default();

        if let Some(local_route) = &self.state.local_route {
            let mut r = local_route.clone();
            r.set_last_seen(SystemTime::now());
            announcement.add_routes(r);
        }

        announcement
    }

    pub fn apply(&mut self, an: &Announcement) {
        let mut changed = false;

        let time_horizon = SystemTime::now() - ROUTE_EXPIRATION_DURATION;

        for new_route in an.routes().iter() {
            let new_route_key = (new_route.group_id(), new_route.server_id());

            if let Some(local_route) = &self.state.local_route {
                if (local_route.group_id(), local_route.server_id()) == new_route_key {
                    continue;
                }
            }

            // We will only accept the new path if it is fresher than the existing route
            // where freshness is defined by when the origin server broadcast this route.
            let should_insert = match self
                .state
                .routes
                .get(&(new_route.group_id(), new_route.server_id()))
            {
                Some(old_route) => {
                    SystemTime::from(new_route.last_seen())
                        > SystemTime::from(old_route.last_seen())
                }
                None => true,
            };

            if should_insert && SystemTime::from(new_route.last_seen()) >= time_horizon {
                self.state.routes.insert(
                    (new_route.group_id(), new_route.server_id()),
                    new_route.as_ref().clone(),
                );

                changed = true;
            }
        }

        self.state.routes.retain(|_, route| {
            if SystemTime::from(route.last_seen()) >= time_horizon {
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

    // pub fn apply(&mut self, an: &Announcement) {
    //     // TODO: Possibly some consideration for a minimum last_used time if
    //     // the route would just get immediately garbage collected upon being
    //     // added

    //     for r in an.routes().iter() {
    //         // If we are a server, never add ourselves to our list
    //         if let Some(ref desc) = self.identity {
    //             if desc.id() == r.desc().id() {
    //                 continue;
    //             }
    //         }

    //         // Add this route if it doesn't already exist or is newer than our
    //         // old entry
    //         let insert = if let Some(old) = self.routes.get(&r.desc().id()) {
    //             SystemTime::from(old.last_used()) <
    // SystemTime::from(r.last_used())         } else {
    //             true
    //         };

    //         if insert {
    //             self.routes.insert(r.desc().id().clone(), r.clone());
    //         }
    //     }
    // }
}
