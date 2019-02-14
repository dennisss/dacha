use super::protos::*;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::SystemTime;
use std::hash::{Hash, Hasher};
use std::borrow::Borrow;


pub type ClusterId = u64;

/// Describes a single server in the cluster using a unique identifier and any information needed to contact it (which may change over time)
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ServerDescriptor {
	pub id: ServerId,
	pub addr: String
}

impl Hash for ServerDescriptor {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.id.hash(state);
    }
}

impl PartialEq for ServerDescriptor {
    fn eq(&self, other: &ServerDescriptor) -> bool {
        self.id == other.id
    }
}
impl Eq for ServerDescriptor {}

// Mainly so that we can look up servers directly by id in the hash sets 
impl Borrow<ServerId> for ServerDescriptor {
	fn borrow(&self) -> &ServerId {
		&self.id
	}
}

impl ServerDescriptor {

	pub fn to_string(&self) -> String {
		self.id.to_string() + " " + &self.addr
	}

	pub fn parse(val: &str) -> std::result::Result<ServerDescriptor, &'static str> {
		let parts = val.split(' ').collect::<Vec<_>>();

		if parts.len() != 2 {
			return Err("Wrong number of parts");
		}

		let id = parts[0].parse::<ServerId>().map_err(|_| "Invalid server id")?;
		let addr = parts[1].to_owned();

		Ok(ServerDescriptor {
			id, addr
		})
	}
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Route {
	pub desc: ServerDescriptor,

	/// Last time this route was retrieved or was observed in an incoming request
	pub last_used: SystemTime
}


/// Thin-serializable state of the server
/// Other details like the cluster_id and from_id are separately managed
#[derive(Serialize, Deserialize, Debug)]
pub struct Announcement {
	// Emitted as a routes vector
	// Merged with the rest of our data 
	pub routes: Vec<Route>
}


/// Represents a single actor in the cluster trying to send/receive messages to/from other agents in the cluster
/// TODO: Eventually refactor to make of the invalid states of this unrepresentable
pub struct NetworkAgent {

	/// Identifies the cluster that these routes and server ids are for
	/// Naturally server ids / addresses are meaningless in a different cluster / ip network, so this ensures metadata isn't being shared between foreign clusters unintentionally
	/// NOTE: Once set, this should never get unset
	pub cluster_id: Option<ClusterId>,

	/// Specified the route to the current server (if we are not acting purely in client mode)
	/// NOTE: May be set only if there is also a cluster_id set
	pub identity: Option<ServerDescriptor>,

	/// All information known about other servers in this network/cluster
	/// For each server this stores the last known location at which it can be reached
	/// 
	/// NOTE: Contains data only if a cluster_id is also set
	/// TODO: Also support an empty record if we believe that the data is invalid (but when we don't won't to clean it up because of )
	/// TODO: Eventually make this private and handle all changes through special methods
	pub routes: HashMap<ServerId, Route>
}

impl NetworkAgent {

	pub fn new() -> Self {
		NetworkAgent {
			cluster_id: None, identity: None, routes: HashMap::new()
		}
	}

	pub fn add_route(&mut self, desc: ServerDescriptor) {
		// Never need to add ourselves
		if let Some(ref our_desc) = self.identity {
			if our_desc.id == desc.id {
				return;
			}
		}

		self.routes.insert(desc.id, Route {
			desc,
			last_used: SystemTime::now()
		});
	}

	pub fn routes(&self) -> &HashMap<ServerId, Route> {
		&self.routes
	}

	pub fn apply(&mut self, an: &Announcement) {

		// TODO: Possibly some consideration for a minimum last_used time if the route would just get immediately garbage collected upon being added

		for r in an.routes.iter() {
			// If we are a server, never add ourselves to our list
			if let Some(ref desc) = self.identity {
				if desc.id == r.desc.id {
					continue;
				}
			}

			// Add this route if it doesn't already exist or is newer than our old entry
			let insert =
				if let Some(old) = self.routes.get(&r.desc.id) {
					old.last_used < r.last_used
				} else {
					true
				};

			if insert {
				self.routes.insert(r.desc.id, r.clone());
			}
		}
	}
}

pub type NetworkAgentHandle = Arc<Mutex<NetworkAgent>>;




