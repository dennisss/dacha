use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::SystemTime;

use common::async_std::sync::Mutex;
use common::errors::*;

use crate::proto::consensus::*;
use crate::proto::routing::*;
use crate::proto::server_metadata::GroupId;

pub type RouteStoreHandle = Arc<Mutex<RouteStore>>;

/// Container of all server-to-server routing information known by the local
/// server.
pub struct RouteStore {
    /// TODO: When a connection times out we want to automatically remove it
    /// from this list.
    routes: HashMap<(GroupId, ServerId), Route>,
    local_route: Option<Route>,
}

impl RouteStore {
    pub fn new() -> Self {
        Self {
            routes: HashMap::new(),
            local_route: None,
        }
    }

    pub fn set_local_route(&mut self, route: Route) {
        self.routes.remove(&(route.group_id(), route.server_id()));
        self.local_route = Some(route);
    }

    /// Looks up routing information for connecting to another server in the
    /// cluster by id. Also marks the request with routing metadata if a
    /// route is fond.
    pub fn lookup(&mut self, group_id: GroupId, server_id: ServerId) -> Option<&Route> {
        // TODO: Use the local route version if available.

        // TODO: Mark the route as recently used.

        self.routes.get(&(group_id, server_id))
    }

    pub fn remote_groups(&self) -> HashSet<GroupId> {
        let mut groups = HashSet::new();
        for (group_id, _) in self.routes.keys().cloned() {
            groups.insert(group_id);
        }

        groups
    }

    pub fn remote_servers(&self, group_id: GroupId) -> HashSet<ServerId> {
        let mut servers = HashSet::new();
        for (cur_group_id, server_id) in self.routes.keys().cloned() {
            if cur_group_id != group_id {
                continue;
            }

            servers.insert(server_id);
        }

        servers
    }

    pub fn serialize(&self) -> Announcement {
        let mut announcement = Announcement::default();

        if let Some(local_route) = &self.local_route {
            let mut r = local_route.clone();
            r.set_last_seen(SystemTime::now());
            announcement.add_routes(r);
        }

        for route in self.routes.values() {
            announcement.add_routes(route.clone());
        }

        announcement
    }

    pub fn apply(&mut self, an: &Announcement) {
        for new_route in an.routes().iter() {
            let new_route_key = (new_route.group_id(), new_route.server_id());

            if let Some(local_route) = &self.local_route {
                if (local_route.group_id(), local_route.server_id()) == new_route_key {
                    continue;
                }
            }

            // We will only accept the new path if it is fresher than the existing route
            // where freshness is defined by when the origin server broadcast this route.
            let should_insert = match self
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
                self.routes.insert(
                    (new_route.group_id(), new_route.server_id()),
                    new_route.clone(),
                );
            }
        }
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
