
use super::errors::*;
use futures::future::*;
use futures::{Future, Stream};

use hyper::{Body, Response};
use hyper::service::service_fn_ok;
use std::collections::{HashMap, HashSet};
use std::time::{SystemTime, Duration};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use rmps::{Deserializer, Serializer};

use super::protos::*;

use std::fs::{File, OpenOptions};
use super::state_machine::StateMachine;


/*
	Atomic machine id creation
	- Make an empty 

*/

/*
	If we assume that cluster membership is a core responsibility of RAFT and not of the overlayed state machine, then we should store the list of server ids to disk as a nice config file

	- Config changes must be atomic on disk
		- The config is essentially a second form of snapshot which we host similar to the state machine (but separate for the sake of simplicity)
		- Could trivially be separate if we wanted it to be 


	Files
	- `/log` <- append-only log (with the exception of compactions which we may either implement as new files )
		- we will generally always hold at most two log files and at most two snapshot files
	- `/config`
		- If the block size is small enough, and we assert that, then the config 
	- 

*/



/// Encapsulates a server's metadata which is persisted to disk
struct MetadataStore {
	// General idea being to just do atomic writes all the time
	// Mix in some checksumming as well and length prefixing

	// Probably not much of a reason to collapse them yet right?

	data: Metadata,
	file: File
}

impl MetadataStore {
	fn open(dir: &Path) {
		let fname = dir.join("meta");

	}

	fn create() {

	}

}


/// Encapsulates a configuration which is persisted to disk
struct ConfigurationStore {

}



struct Server {


}

////////////////////////

/*
type ServerStateHandle = Arc<Mutex<ServerState>>;

impl Default for ServerState {
	fn default() -> Self {
		ServerState {
			cluster: Vec::new(),
			id: 0, // < 0 implies not in any cluster even 
			last_time: SystemTime::now(),
			persistent: ServerPersistentState::default(),
			shared: ServerSharedState::default(),
			role: ServerRole::Follower(ServerFollowerState {
				election_timeout: ServerState::new_election_timeout()
			})
		}
	}
}

*/

// TODO: Only count votes if we haven't yet transitioned yet since the time we started the vote

/*
	Additional state:
	- For followers,
		- Time at which the last heartbeat was received
	- For leaders
		- Time at which the lat heartbeat was sent
	- For candidates
		- Time at which the election started
	

	General assumptions
	- For now we assume that the number and locations of all servers is well known
	- Long term, we will start with a single server having it's own id 

*/
/*
	All RPCs we need
	- /append_entries
	- /request_vote
	- /install_snapshot

*/

/*
	Membership change
	- Abstraction on append_entries

	Leadership
*/

/*
	- If not already commited, commit C(old + new) to all old + new servers
	- Then it shall commit C(new) to new servers
	
	- After this point, if any server gets a message from an id that is not in it's set, it can just reject it
		- Possibly iss


	Other scenarios
	- Server startup
		- Server always starts completely idle and in a mode that would reject external requests
		- If we have configuration on disk already, then we can use that
		- If we start with a join cli flag, then we can:
			- Ask the cluster to create a new unique machine id (we could trivially use an empty log entry and commit that to create a new id) <- Must make sure this does not conflict with the master's id if we make many servers before writing other data
	
		- If we are sent a one-time init packet via http post, then we will start a new cluster on ourselves

	- Initializing a cluster for one node
		- by default a server will have zero servers in it's config and will refuse to cycle at all
		- if it is 

*/
