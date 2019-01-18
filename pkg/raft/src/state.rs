use super::protos::ServerId;
use std::time::Duration;
use std::collections::{HashMap, HashSet};


#[derive(Clone, Debug)]
pub struct ServerLeaderStateEntry {
	/// for each server, index of the next log entry to send to that server (initialized to leader last log index + 1)
	pub next_index: u64,

	/// for each server, index of highest log entry known to be replicated on server (initialized to 0, increases monotonically)
	pub match_index: u64
}

#[derive(Clone, Debug)]
pub struct ServerFollowerState {
	pub election_timeout: Duration,

	/// Id of the last leader we have been pinged by. Used to cache the location of the current leader for the sake of proxying requests and client hinting 
	pub last_leader_id: Option<ServerId>
}

#[derive(Clone, Debug)]
pub struct ServerCandidateState {
	/// Similar to the follower one this is when we should start the next election all over again
	pub election_timeout: Duration,

	/// All the votes we have received so far from other servers
	/// TODO: This would also be a good time to pre-warm the leader states based on this 
	pub votes_received: HashSet<ServerId>
}

/// Volatile state on leaders:
/// (Reinitialized after election)
#[derive(Clone, Debug)]
pub struct ServerLeaderState {
	pub servers: HashMap<ServerId, ServerLeaderStateEntry>
}

impl ServerLeaderState {
	pub fn new() -> Self {
		// TODO: Ideally initialize with our expected capacity (or if we keep it in sync with the list in the config, we could make this a fixed length vector (performing swaps from the end of the list whenever we want to remove a server))
		ServerLeaderState {
			servers: HashMap::new()
		}
	}
}

#[derive(Clone, Debug)]
pub enum ServerState {
	Follower(ServerFollowerState),
	Candidate(ServerCandidateState),
	Leader(ServerLeaderState),
	//Learner(ServerFollowerState)
}
