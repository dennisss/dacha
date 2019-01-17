use super::protos::ServerId;
use std::time::Duration;
use std::collections::{HashMap, HashSet};


pub struct ServerLeaderStateEntry {
	/// for each server, index of the next log entry to send to that server (initialized to leader last log index + 1)
	pub next_index: u64,

	/// for each server, index of highest log entry known to be replicated on server (initialized to 0, increases monotonically)
	pub match_index: u64
}

pub struct ServerFollowerState {
	pub election_timeout: Duration,

	/// Id of the last leader we have been pinged by. Used to cache the location of the current leader for the sake of proxying requests and client hinting 
	pub last_leader_id: Option<ServerId>
}

pub struct ServerCandidateState {
	/// Similar to the follower one this is when we should start the next election all over again
	pub election_timeout: Duration,

	/// All the votes we have received so far from other servers
	pub votes_received: HashSet<ServerId>
}

/// Volatile state on leaders:
/// (Reinitialized after election)
pub struct ServerLeaderState {
	pub servers: HashMap<ServerId, ServerLeaderStateEntry>
}

pub enum ServerState {
	Follower(ServerFollowerState),
	Candidate(ServerCandidateState),
	Leader(ServerLeaderState),
	//Learner(ServerFollowerState)
}
