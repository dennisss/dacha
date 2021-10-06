use std::collections::HashMap;
use std::sync::Arc;
use std::time::SystemTime;

use common::async_std::sync::Mutex;
use common::errors::*;

use crate::proto::consensus::*;
use crate::proto::routing::*;
use crate::proto::server_metadata::GroupId;

// In our RPC, these will contain a serialized ServerDescriptor representing
// which server is sending the request and who is the designated receiver
// NOTE: All key names must be lowercase as they may get normalized in http2
// transport anyway and case may not get preversed on the other side
const FROM_KEY: &str = "raft-from";
const TO_KEY: &str = "raft-to";
const GROUP_ID_KEY: &str = "raft-group-id";

/// Represents a single actor in the cluster trying to send/receive messages
/// to/from other agents in the cluster
/// TODO: Eventually refactor to make of the invalid states of this
/// unrepresentable
pub struct NetworkAgent {
    /// Identifies the cluster that these routes and server ids are for
    /// Naturally server ids / addresses are meaningless in a different cluster
    /// / ip network, so this ensures metadata isn't being shared between
    /// foreign clusters unintentionally
    /// NOTE: Once set, this should never get unset
    pub group_id: Option<GroupId>,

    /// Specified the route to the current server (if we are not acting purely
    /// in client mode)
    /// NOTE: May be set only if there is also a group_id set
    pub identity: Option<ServerDescriptor>,

    /// All information known about other servers in this network/cluster
    /// For each server this stores the last known location at which it can be
    /// reached
    ///
    /// NOTE: Contains data only if a group_id is also set
    /// TODO: Also support an empty record if we believe that the data is
    /// invalid (but when we don't won't to clean it up because of )
    routes: HashMap<ServerId, Route>,
}

impl NetworkAgent {
    pub fn new() -> Self {
        NetworkAgent {
            group_id: None,
            identity: None,
            routes: HashMap::new(),
        }
    }

    pub fn add_route(&mut self, desc: ServerDescriptor) {
        // Never need to add ourselves
        if let Some(ref our_desc) = self.identity {
            if our_desc.id() == desc.id() {
                return;
            }
        }

        let mut route = Route::default();
        route.set_desc(desc.clone());
        route.set_last_used(SystemTime::now());

        self.routes.insert(desc.id(), route);
    }

    /// Looks up routing information for connecting to another server in the
    /// cluster by id. Also marks the request with routing metadata if a
    /// route is fond.
    pub fn lookup(
        &mut self,
        id: ServerId,
        context: &mut rpc::ClientRequestContext,
    ) -> Option<&Route> {
        self.routes.get_mut(&id).map(|e| {
            context
                .metadata
                .add_text(TO_KEY, &e.desc().to_string())
                .unwrap();

            e.set_last_used(SystemTime::now());
            e as &Route
        })
    }

    pub fn routes(&self) -> &HashMap<ServerId, Route> {
        &self.routes
    }

    pub fn serialize(&self) -> Announcement {
        let mut announcement = Announcement::default();
        for route in self.routes.values() {
            announcement.add_routes(route.clone());
        }

        announcement
    }

    pub fn apply(&mut self, an: &Announcement) {
        // TODO: Possibly some consideration for a minimum last_used time if
        // the route would just get immediately garbage collected upon being
        // added

        for r in an.routes().iter() {
            // If we are a server, never add ourselves to our list
            if let Some(ref desc) = self.identity {
                if desc.id() == r.desc().id() {
                    continue;
                }
            }

            // Add this route if it doesn't already exist or is newer than our
            // old entry
            let insert = if let Some(old) = self.routes.get(&r.desc().id()) {
                SystemTime::from(old.last_used()) < SystemTime::from(r.last_used())
            } else {
                true
            };

            if insert {
                self.routes.insert(r.desc().id().clone(), r.clone());
            }
        }
    }

    pub fn append_to_request_context(&self, context: &mut rpc::ClientRequestContext) -> Result<()> {
        if let Some(c) = self.group_id {
            context
                .metadata
                .add_text(GROUP_ID_KEY, &c.value().to_string())?;
        }

        if let Some(ref id) = self.identity {
            context.metadata.add_text(FROM_KEY, &id.to_string())?;
        }

        Ok(())
    }

    pub fn process_response_metadata(
        &mut self,
        context: &rpc::ClientResponseContext,
    ) -> Result<()> {
        if let Some(v) = context.metadata.head_metadata.get_text(GROUP_ID_KEY)? {
            let cid_given = v.parse()?;

            if let Some(cid) = self.group_id {
                if cid != cid_given {
                    return Err(err_msg("Received response with mismatching group_id"));
                }
            } else {
                self.group_id = Some(cid_given);
            }
        }

        if let Some(v) = context.metadata.head_metadata.get_text(FROM_KEY)? {
            // TODO: Disambiguate with the protobuf parse() method!
            let desc = match v.parse::<ServerDescriptor>() {
                Ok(v) => v,
                Err(_) => return Err(err_msg("Invalid 'From' metadata received")),
            };

            // TODO: If we originally requested this server under a
            // different id, it would be nice to erase that other record or
            // tombstone it

            self.add_route(desc);
        }

        Ok(())
    }
}

pub type NetworkAgentHandle = Arc<Mutex<NetworkAgent>>;

pub struct ServerRequestRoutingContext {
    /// Whether or not the received request is known to be in the same cluster
    /// as us.
    pub verified_cluster: bool,

    /// Whether or not we can verify that the this server is the correct
    /// recipient of this request.
    pub verified_recipient: bool,
}

impl ServerRequestRoutingContext {
    pub async fn create(
        network_agent: &Mutex<NetworkAgent>,
        request_context: &rpc::ServerRequestContext,
        response_context: &mut rpc::ServerResponseContext,
    ) -> Result<Self> {
        let mut agent = network_agent.lock().await;
        let our_group_id = agent.group_id.unwrap();
        let our_ident = agent.identity.as_ref().unwrap().clone();

        response_context
            .metadata
            .head_metadata
            .add_text(GROUP_ID_KEY, &our_group_id.to_string())?;
        response_context
            .metadata
            .head_metadata
            .add_text(FROM_KEY, &our_ident.to_string())?;

        // We first validate the cluster id because it must be valid for us to trust any
        // of the other routing data
        let verified_cluster = if let Some(h) = request_context.metadata.get_text(GROUP_ID_KEY)? {
            let cid = h
                .parse::<GroupId>()
                .map_err(|_| rpc::Status::invalid_argument("Invalid cluster id"))?;

            if cid != our_group_id {
                // TODO: This is a good reason to send back our group_id so that
                // they can delete us as a route
                return Err(rpc::Status::invalid_argument("Mismatching cluster id").into());
            }

            true
        } else {
            false
        };

        // Record who sent us this message
        // TODO: Should receiving a message from one's self be an error?
        if let Some(h) = request_context.metadata.get_text(FROM_KEY)? {
            if !verified_cluster {
                return Err(rpc::Status::invalid_argument(
                    "Received From header without a cluster id check",
                )
                .into());
            }

            let desc = h.parse::<ServerDescriptor>()?;
            agent.add_route(desc);
        }

        // Verify that we are the intended recipient of this message
        let verified_recipient = if let Some(h) = request_context.metadata.get_text(TO_KEY)? {
            if !verified_cluster {
                return Err(rpc::Status::invalid_argument(
                    "Received To header without a cluster id check",
                )
                .into());
            }

            let addr = h
                .parse::<ServerDescriptor>()
                .map_err(|_| rpc::Status::invalid_argument("Invalid To descriptor"))?;

            if addr.id() != our_ident.id() {
                // Bail out. The client should adjust its routing info based on the
                // identity we return back in response metadata.
                return Err(rpc::Status::invalid_argument("Not the intended recipient").into());
            }

            true
        } else {
            false
        };

        Ok(Self {
            verified_cluster,
            verified_recipient,
        })
    }

    pub fn assert_verified(&self) -> Result<()> {
        if !self.verified_recipient {
            return Err(rpc::Status::invalid_argument(
                "Cluster and receipient must be specified to make this request",
            )
            .into());
        }

        Ok(())
    }
}
