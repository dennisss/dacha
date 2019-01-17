
use super::errors::*;
use super::protos::*;
use super::rpc;
use super::state::*;
use futures::future::*;
use futures::{Future, Stream};

use std::collections::{HashMap, HashSet};
use std::time::{SystemTime, Duration};
use std::sync::{Arc, Mutex};

use std::fs::{File, OpenOptions};
use super::state_machine::StateMachine;
use std::thread;


/// At some random time in this range of milliseconds, a follower will become a candidate if no 
const ELECTION_TIMEOUT: (u64, u64) = (400, 800); 

/// If the leader doesn't send anything else within this amount of time, then it will send an empty heartbeat to all followers (this default value would mean around 5 heartbeats each second)
const HEARTBEAT_TIMEOUT: u64 = 200;


pub struct Server<S> where S: StateMachine + Send + 'static {
	meta: Metadata,
	config: Configuration,
	log: Vec<LogEntry>,

	/// Index of the last log entry known to be commited
	/// NOTE: It is not generally necessary to store this, and can be re-initialized always to at least the index of the last applied entry in config or log
	commit_index: Option<u64>,

	/// For followers, this is the last time we have received a heartbeat from the leader
	/// For candidates, this is the time at which they started their election
	/// For leaders, this is the last time we sent any rpc to our followers
	last_time: SystemTime,

	// Basically this is the persistent state stuff
	state: ServerState,

	// Then we also have in-memory volatile state

	state_machine: Arc<Mutex<S>> // NOTE: We will eventually just template the type of it
}

/*
	In order to make a server, we must at least have a server id 
	- First and for-most, if there already exists a file on disk with metadata, then we should use that
	- Otherwise, we must just block until we have a machine id by some other method
		- If an existing cluster exists, then we will ask it to make a new cluster id
		- Otherwise, the main() script must wait for someone to bootstrap us and give ourselves id 1

*/

impl<S: StateMachine + Send + 'static> Server<S> {

	// Generally we will need to have a configuration available and such
	// If this machine does not have a machine id, then one must be created before starting the server (either by a bootstrap process or by obtaining a new id from an existing cluster)
	pub fn new() -> Server<super::state_machine::MemoryKVStateMachine> {
		// For now this is for a single server with well known id (but no servers in the cluster)
		Server {
			meta: Metadata {
				server_id: 1,
				current_term: 0,
				voted_for: None
			},
			config: Configuration {
				last_applied: 0, // TODO: Convert to an Option
				members: HashSet::new(),
				learners: HashSet::new()
			},
			log: Vec::new(),

			commit_index: None,

			last_time: SystemTime::now(),

			state: ServerState::Follower(ServerFollowerState {
				election_timeout: Self::new_election_timeout(),
				last_leader_id: None
			}),

			state_machine: Arc::new(Mutex::new(super::state_machine::MemoryKVStateMachine::new()))
		}
	}

	// Should ideally start a new thread that 
	pub fn start(inst: Arc<Mutex<Server<S>>>) {

		let inst2 = inst.clone();

		thread::spawn(move || {
			let inst = inst.clone();

			rpc::run_server(4000, inst);
		});

		// General loop for managing the server and maintaining leadership, etc.
		thread::spawn(move || {
			loop {
				let mut server = inst2.lock().unwrap();
				server.cycle();
				
				// If sleep is required, we should run a conditional variable that wakes us up an recycles as needed
			}
		});

		// TODO: Finally if possible we should attempt to broadcast our ip address to other servers so they can rediscover us

	}

	fn cycle(&mut self) -> Option<Duration> {
		let time = SystemTime::now();
		let elapsed = time.duration_since(self.last_time).unwrap_or(Duration::from_millis(0));

		// TODO: If there are no members in the cluster, then this trivially has nothing to do until we get added to someone's cluster or someone will bootstrap us

		match self.state {
			ServerState::Follower(s) => {
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
			ServerState::Candidate(s) => {
				// TODO: This will basically end up being the same exact precedure as for folloders
				// Possibly some logic for retring requests
			},
			ServerState::Leader(s) => {
				// If we have gone too long without a hearbeart, send one
				// Also, if we have followers that are lagging behind, it would be a good time to update them if no requests are active for them
			}
		};

		None
	}


	/*
		- Another consideration would be to maintain exactly once semantics when we are sneding a command to a server

		- Log cabin generally uses the following naming conventions
			- The client runs a 'command' on the leader's server
			- The consensus module can 'replicate()' a log entry to other consensus modules
			

		- Generally yes, the consensus module is separate


		In LogCabin, applying entries is essentially secondary to consensus module
			- The state machine simply asynchronously applies entries eventually once the consensus module has accepted them

			- So yes, general idea is to decouple the consensus module from the log and from the state machine


	*/

	/// Assuming that this is running on the leader, this will 
	pub fn create_entries(entries: &[LogEntry]) {
		// Fail if we are not the leader

		// Generally 


	}


	fn start_election(&mut self) {
		// Basically must be run and presisted
		self.meta.current_term += 1;
		self.meta.voted_for = Some(self.meta.server_id);

		// Really not much point to generalizing it mainly because it still requires that we have much more stuff
		self.state = ServerState::Candidate(ServerCandidateState {
			election_timeout: Self::new_election_timeout(),
			votes_received: HashSet::new()
		});

		// Send up a bunch of RPCSs

		let req = RequestVoteRequest {
			term: self.meta.current_term,
			candidate_id: self.meta.server_id,

			// TODO: Grab from the log entries (indexes starting at 1)
			last_log_index: 0,
			last_log_term: 0
		};

		let sent = self.config.members.iter().filter(|s| {
			s.id != self.meta.server_id
		})
		.map(|s| {

			// NOTE: We should be able to handle individual but literally as soon as we hit a majority we can respond to the request
			// In other cases, we should still maintain a casual timeout

			rpc::call_request_vote(s, &req)
			.and_then(|resp| {

				// TODO: Only count votes if we haven't yet transitioned yet since the time we started the vote

				futures::future::ok(())
			})

		}).collect::<Vec<_>>();

		// Once all requests are completed, if we haven't yet transitioned to another term, then we can probably then check if all votes were successful (obviously no point in checking until done)
		// ^ Possibly make the votes list scoped to only this function to not require locking 



		let f = futures::future::join_all(sent)
		.map(|_| {
			()
		})
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



	fn append_entries_impl(&mut self, req: AppendEntriesRequest) -> bool {
		if req.term < self.meta.current_term {
			return false;
		}

		if (req.prev_log_index as usize) > self.log.len() ||
			self.log[req.prev_log_index as usize].term != req.prev_log_term {
			
			return false;
		}

		// Assert that the entries we were sent are in sorted order with no repeats
		// NOTE: It is also generally infeasible for 
		for i in 0..(req.entries.len() - 1) {
			// XXX: This only makes sense if we start sending an index with each entry a well
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


impl<S: StateMachine + Send + 'static> rpc::Server for Server<S> {
	fn request_vote(&mut self, req: RequestVoteRequest) -> Result<RequestVoteResponse> {

		// TODO: If we grant a vote to one server and then we see another server also ask us for a vote (and we reject it but that other server has a higher term, should we still update our current_term with that one we just rejected)

		let granted = {
			if req.term < self.meta.current_term {
				false
			}
			else {
				// TODO: Verify candidate log at least as up to date as our log 

				match self.meta.voted_for {
					Some(id) => {
						id == req.candidate_id
					},
					None => true
				}
			}
		};

		if granted {
			self.meta.voted_for = Some(req.candidate_id);
		}

		// XXX: Persist to storage before responding

		// NOTE: Much simpler for term to start at 0 right?
		Ok(RequestVoteResponse {
			term: self.meta.current_term,
			vote_granted: granted
		})
	}

	
	fn append_entries(&mut self, req: AppendEntriesRequest) -> Result<AppendEntriesResponse> {
		let success = self.append_entries_impl(req);

		if success {
			// In this case we can also update our last time, trigger the condvar and if we are not already a follower, we can become a follower
		}
		
		// XXX: Old and not correct
		Ok(AppendEntriesResponse {
			term: 0,
			success: false
		})
	}

}

