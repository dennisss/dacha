use super::protos::ServerId;
use std::time::{Duration, Instant};
use std::collections::{HashMap, HashSet};

/*
	Ideally we would generalize away the flow control aspect of it
	- Such that we can poll someone to determine if we can send another message
		- Because many ranges could be running at once, we must be able to prioritize timeout heartbeats ahead of log replication
*/


#[derive(Clone, Debug)]
pub struct ServerProgress {
	/// Index of the next log entry we should send to this server (starts at last leader index + 1)
	pub next_index: u64,

	/// Index of the highest entry known to be replicated to this server
	/// TODO: These can be long term persisted even after we fall out of being leader as long as we only persist the largest match_index for a client that is >= the commited_index at the time of persisting the value
	pub match_index: u64,

	/// Last time a heartbeat was successfully sent and received (the times in this tuple represent the corresponding send/received times for a single request)
	/// TODO: Per section 6.2 of the thesis, a leader should ideally step down once not receiving a heartbeat for an entire election cycle (we must make sure that stepping down cancels any pending waiters if appropriate)
	pub last_heartbeat: Option<(Instant, Instant)>,

	/// Time at which we sent out the last request to this server
	/// Main issue being that this will become trickier in the case of pipelined messages
	pub last_sent: Option<Instant>,

	/// Whether or not we are currently waiting for a response on an active request
	pub request_pending: bool
}

impl ServerProgress {

	// NOTE: Upon becoming leader, we will trivially have heartbeats from at least a quorum of servers based on the RequestVotes
	// - This will allow us to immediately serve read requests in many cases

	/// Create a new progress entry given the leader's last log index
	pub fn new(last_log_index: u64) -> Self {
		ServerProgress {
			next_index: last_log_index + 1,
			match_index: 0,
			last_heartbeat: None,
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
