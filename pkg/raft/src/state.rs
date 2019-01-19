use super::protos::ServerId;
use std::time::{Duration, Instant};
use std::collections::{HashMap, HashSet};


#[derive(Clone, Debug)]
pub struct ServerProgress {
	/// for each server, index of the next log entry to send to that server (initialized to leader last log index + 1)
	/// 
	/// Index of the next log entry we should send to this server
	pub next_index: u64,

	/// Index of the highest entry known to be replicated to this server
	pub match_index: u64,

	/// Time at which we sent out the last request to this server
	pub last_sent: Option<Instant>,

	/// Whether or not we are currently waiting for a response on an active request
	pub request_pending: bool
}

impl ServerProgress {

	/// Create a new progress entry given the leader's last log index
	pub fn new(last_log_index: u64) -> Self {
		ServerProgress {
			next_index: last_log_index + 1,
			match_index: 0,
			last_sent: None, // This will force the leader to send initial heartbeats to all servers upon being elected
			request_pending: false
		}
	}
}

#[derive(Clone, Debug)]
pub struct ServerFollowerState {
	pub election_timeout: Duration,

	/// Id of the last leader we have been pinged by. Used to cache the location of the current leader for the sake of proxying requests and client hinting 
	pub last_leader_id: Option<ServerId>,

	/// Last time we received a message from the leader (or when we first transitioned to become a follower)
	pub last_heartbeat: Instant
}

#[derive(Clone, Debug)]
pub struct ServerCandidateState {
	/// Time at which this candidate started its election
	pub election_start: Instant,

	/// Similar to the follower one this is when we should start the next election all over again
	pub election_timeout: Duration,

	/// All the votes we have received so far from other servers
	/// TODO: This would also be a good time to pre-warm the leader states based on this 
	pub votes_received: HashSet<ServerId>,

	/// Defaults to false, if we receive a vote rejection in a valid response, we will mark this as true to indicate that this current term has contention from other nodes
	/// So if we don't win the election, we must bump the term
	pub some_rejected: bool

}

/// Volatile state on leaders:
/// (Reinitialized after election)
#[derive(Clone, Debug)]
pub struct ServerLeaderState {
	pub servers: HashMap<ServerId, ServerProgress>
}


#[derive(Clone, Debug)]
pub enum ServerState {
	Follower(ServerFollowerState),
	Candidate(ServerCandidateState),
	Leader(ServerLeaderState)
}
