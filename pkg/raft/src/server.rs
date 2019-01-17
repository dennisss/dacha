


use hyper::{Body, Response, Server};
use hyper::rt::Future;
use hyper::service::service_fn_ok;
use std::collections::{HashMap, HashSet};
use std::time::{SystemTime, Duration};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use rmps::{Deserializer, Serializer};

use super::protos::*;


/*
	If we assume that cluster membership is a core responsibility of RAFT and not of the overlayed state machine, then we should store the list of server ids to disk as a nice config file

	- Config changes must be atomic on disk


	Files
	- `/log` <- append-only log (with the exception of compactions which we may either implement as new files )
		- we will generally always hold at most two log files and at most two snapshot files
	- `/config`
		- If the block size is small enough, and we assert that, then the config 
	- 

*/

use std::fs::{File, OpenOptions};


/// Encapsulates a server's metadata which is persisted to disk
struct MetadataStore {
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

struct LogStore {

}


/// Encapsulates a configuration which is persisted to disk
struct ConfigurationStore {

}




/*
/// Persistent state on all servers:
/// (Updated on stable storage before responding to RPCs)
struct ServerPersistentState {	
	/// log entries; each entry contains command for state machine, and term when entry was received by leader (first index is 1)
	pub log: Vec<LogEntry>
}

impl Default for ServerPersistentState {
	fn default() -> Self {
		ServerPersistentState {
			current_term: 0,
			voted_for: None,
			log: Vec::new()
		}
	}
}
*/

use super::state_machine::StateMachine;

struct Server {
	meta: MetadataStore,
	config: ConfigurationStore,
	log: LogStore,

	/// Index of the last log entry known to be commited
	/// NOTE: It is not generally necessary to store this, and can be re-initialized always to at least the index of the last applied entry in config or log
	commit_index: Option<u64>,

	state_machine: Arc<Mutex<StateMachine>>
}



/// Volatile state on all servers:
struct ServerSharedState {
	/// index of highest log entry known to be committed (initialized to 0, increases monotonically)
	pub commit_index: Option<u64>, // < XXX: Will be stored as well (this is always at least the same as the lastApplied index)
	// ^ Strictly always at least as large as last_applied (therefore doesn't really need to be super consistent)

	/// index of highest log entry applied to state machine (initialized to 0, increases monotonically)
	/// Generally no real point in holding on to this as we can assume that we have a server
	pub last_applied: Option<u64>
}

impl Default for ServerSharedState {
	fn default() -> Self {
		ServerSharedState {
			commit_index: None, // < Really doesn't need to be saved to disk
			last_applied: None // < Will be saved to disk, but only by the state machine
		}
	}
}

struct ServerLeaderStateEntry {
	/// for each server, index of the next log entry to send to that server (initialized to leader last log index + 1)
	pub next_index: u64,

	/// for each server, index of highest log entry known to be replicated on server (initialized to 0, increases monotonically)
	pub match_index: u64
}

struct ServerFollowerState {
	election_timeout: Duration,

	/// Id of the last leader we have been pinged by. Used to cache the location of the current leader for the sake of proxying requests and client hinting 
	last_leader_id: Option<ServerId>
}

struct ServerCandidateState {
	/// similar to the follower one this is when we should start the next election all over again
	election_timeout: Duration,

	/// All the votes we have received so far from other servers
	votes_received: HashSet<ServerId>
}

/// Volatile state on leaders:
/// (Reinitialized after election)
struct ServerLeaderState {
	pub servers: HashMap<u64, ServerLeaderStateEntry>
}

/// Ideally a separate 
enum ServerRole {
	Follower(ServerFollowerState),
	Candidate(ServerCandidateState),
	Leader(ServerLeaderState)
}

struct ServerDesc {
	pub id: u64,
	pub addr: String
}

type ServerStateHandle = Arc<Mutex<ServerState>>;

struct ServerState {

	/// All members of the cluster
	pub cluster: Vec<ServerDesc>,

	pub id: ServerId,

	/// For followers, this is the last time we have received a heartbeat from the leader
	/// For candidates, this is the time at which they started their election
	/// For leaders, this is the last time we sent any rpc to our followers
	pub last_time: SystemTime,

	pub persistent: ServerPersistentState,

	pub shared: ServerSharedState,

	pub role: ServerRole

}

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

use futures::future::*;
use futures::Stream;


fn make_request<'a, Req, Res>(path: &'static str, req: Req)
	-> impl Future<Item=Res, Error=hyper::Error>
	where Req: Serialize,
		  Res: Deserialize<'a>	
{
	let client = hyper::Client::new();

}

impl ServerState {

	pub fn cycle(&mut self) -> Option<Duration> {
		let time = SystemTime::now();
		let elapsed = time.duration_since(self.last_time).unwrap_or(Duration::from_millis(0));

		match self.role {
			ServerRole::Follower(s) => {
				if elapsed >= s.election_timeout {
					self.start_election();					
				}
				else {
					// Otherwise sleep until the next election
					// The preferred method here will be to wait on the conditional variable if 
					// We will probably generalize to passing around Arc<Server> with the 
					return Some(s.election_timeout - elapsed);
				}
			},
			ServerRole::Candidate(s) => {
				// TODO: This will basically end up being the same exact precedure as for folloders
				// Possibly some logic for retring requests
			},
			ServerRole::Leader(s) => {
				// If we have gone too long without a hearbeart, send one
			}
		};

		None
	}

	pub fn start_election(&mut self) {
		self.persistent.current_term += 1;
		self.persistent.voted_for = Some(self.id);
		self.role = ServerRole::Candidate(ServerCandidateState {
			election_timeout: ServerState::new_election_timeout(),
			votes_received: HashSet::new()
		});

		// Send up a bunch of RPCSs

		let req = RequestVoteRequest {
			term: self.persistent.current_term,
			candidate_id: self.id,

			// TODO: Grab from the log entries (indexes starting at 1)
			last_log_index: 0,
			last_log_term: 0
		};

		let data = {
			let mut buf = Vec::new();
			req.serialize(&mut Serializer::new(&mut buf)).unwrap();
			bytes::Bytes::from(buf)
		};


		let sent = self.cluster.iter().filter(|s| {
			s.id != self.id
		})
		.map(|s| {
			let data = data.clone();

			let client = hyper::Client::new();

			let r = hyper::Request::builder()
				.uri(format!("{}/request_vote", s.addr))
				.body(Body::from(data))
				.unwrap();

			client
			.request(r)
			.and_then(|resp| {

				if !resp.status().is_success() {
					return futures::future::err(format!("RPC call failed with code: {}", resp.status().as_u16()))

					// An error occured, but we probably still want to grab the whole body 
				}

				let body = resp.into_body();

				resp.into_body()
				.fold(Vec::new(), |mut buf, c| {
					buf.extend_from_slice(&c);
					ok(buf)
				})
				.and_then(|buf| {
					let mut de = Deserializer::new(&buf[..]);
					let ret = Deserialize::deserialize(&mut de).unwrap();

					// 

					futures::future::ok(ret)
				})



				/*
					assert_eq!((42, "the Answer".to_owned()), Deserialize::deserialize(&mut de).unwrap());
				*/
				

				// Parse the respone 


				// TODO: Only count votes if we haven't yet transitioned yet since the time we started the vote

				


				// 

				futures::future::ok(())
			})

		}).collect::<Vec<_>>();

		// Once all requests are completed, if we haven't yet transitioned to another term, then we can probably then check if all votes were successful (obviously no point in checking until done)
		// ^ Possibly make the votes list scoped to only this function to not require locking 



		let f = futures::future::join_all(sent)
		.map_err(|e| {
			eprintln!("Error while requesting votes: {:?}", e);
			()
		});

		// TODO: We should chain on some promise holding one side of a channel so that we can cancel this entire request later if we end up needing to 

		tokio::spawn(f);



		// Would be good for this to have it's on 

	}

	fn new_election_timeout() -> Duration {
		Duration::from_millis(200)
	}

	pub fn leader_send_heartbeat(&mut self) {

	}

	pub fn on_request_vote(&mut self, req: &RequestVoteRequest) -> RequestVoteResponse {

		// TODO: If we grant a vote to one server and then we see another server also ask us for a vote (and we reject it but that other server has a higher term, should we still update our current_term with that one we just rejected)

		let granted = {
			if req.term < self.persistent.current_term {
				false
			}
			else {
				// TODO: Verify candidate log at least as up to date as our log 

				match self.persistent.voted_for {
					Some(id) => {
						id == req.candidate_id
					},
					None => true
				}
			}
		};

		if granted {
			self.persistent.voted_for = Some(req.candidate_id);
		}

		// XXX: Persist to storage before responding

		RequestVoteResponse {
			term: self.persistent.current_term,
			vote_granted: granted
		}
	}

	pub fn on_append_entries(&mut self, req: &AppendEntriesRequest) -> AppendEntriesResponse {

		let success = self.on_append_entries_run(req);

		if success {
			// In this case we can also update our last time, trigger the condvar and if we are not already a follower, we can become a follower
		}

	}

	pub fn on_append_entries_run(&mut self, req: &AppendEntriesRequest) -> bool {
		if req.term < self.persistent.current_term {
			return false;
		}

		if (req.prev_log_index as usize) > self.persistent.log.len() ||
			self.persistent.log[req.prev_log_index].term != req.prev_log_term {
			
			return false;
		}

		// Assert that the entries we were sent are in sorted order with no repeats
		// NOTE: It is also generally infeasible for 
		for i in 0..(req.entries.len() - 1) {
			if req.entries[i + 1] <= req.entries[i] {
				eprintln!("Received unsorted or duplicate log entries");
				return false;
			}
		}

		// Delete conflicting entries and all other it
		for e in req.entries.iter() {

		}


		// delete existing entries in conflict with new ones
		// basically this may require an undo operation on the (but we dont commit anything until we actually run it, so this should never be an issue)




		// append new log entries not already in the log

		true
	}


}



// General loop for managing the server and maintaining leadership, etc.
fn run_server(state_handle: Arc<Mutex<ServerState>>) {

	loop {
		let mut state = state_handle.lock().unwrap();
		state.cycle();
		

		// If sleep is required, we should run a conditional variable that wakes us up an recycles as needed

	}


}


/// At some random time in this range of milliseconds, a follower will become a candidate if no 
const ELECTION_TIMEOUT: (u64, u64) = (150, 300); 


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

// TODO: Move to the protocol set (will be represented as only Add and Remove operations)
// That way the config is trivially replicatable
struct ConfigChange {
	members: Vec<ServerId>
}

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
