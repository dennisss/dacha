use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::SystemTime;

use common::condvar::{Condvar, CondvarGuard};

use crate::proto::consensus::*;
use crate::proto::routing::*;
use crate::proto::server_metadata::GroupId;

/*
A RouteResolver can have a child task which waits for changes and notifies all
*/

/// Container of all server-to-server routing information known by the local
/// server.
#[derive(Clone)]
pub struct RouteStore {
    state: Arc<Condvar<State>>,
}

struct State {
    /// TODO: When a connection times out we want to automatically remove it
    /// from this list.
    routes: HashMap<(GroupId, ServerId), Route>,
    local_route: Option<Route>,
}

impl RouteStore {
    pub fn new() -> Self {
        Self {
            state: Arc::new(Condvar::new(State {
                routes: HashMap::new(),
                local_route: None,
            })),
        }
    }

    pub async fn lock<'a>(&'a self) -> RouteStoreGuard<'a> {
        RouteStoreGuard {
            state: self.state.lock().await,
        }
    }
}

pub struct RouteStoreGuard<'a> {
    state: CondvarGuard<'a, State, ()>,
}

impl<'a> RouteStoreGuard<'a> {
    pub fn set_local_route(&mut self, route: Route) {
        self.state
            .routes
            .remove(&(route.group_id(), route.server_id()));
        self.state.local_route = Some(route);
    }

    /// Looks up routing information for connecting to another server in the
    /// cluster by id. Also marks the request with routing metadata if a
    /// route is fond.
    pub fn lookup(&mut self, group_id: GroupId, server_id: ServerId) -> Option<&Route> {
        // TODO: Use the local route version if available.

        // TODO: Mark the route as recently used.

        self.state.routes.get(&(group_id, server_id))
    }

    pub fn remote_groups(&self) -> HashSet<GroupId> {
        let mut groups = HashSet::new();
        for (group_id, _) in self.state.routes.keys().cloned() {
            groups.insert(group_id);
        }

        groups
    }

    pub fn remote_servers(&self, group_id: GroupId) -> HashSet<ServerId> {
        let mut servers = HashSet::new();
        for (cur_group_id, server_id) in self.state.routes.keys().cloned() {
            if cur_group_id != group_id {
                continue;
            }

            servers.insert(server_id);
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

            if should_insert {
                self.state.routes.insert(
                    (new_route.group_id(), new_route.server_id()),
                    new_route.clone(),
                );

                changed = true;
            }
        }

        if changed {
            self.state.notify_all();
        }
    }

    pub async fn wait(self) {
        self.state.wait(()).await
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
