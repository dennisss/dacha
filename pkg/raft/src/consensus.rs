
use super::errors::*;
use super::protos::*;
use super::rpc;
use super::state::*;
use super::log::*;
use futures::future::*;
use futures::{Future, Stream};

use std::collections::{HashMap, HashSet};
use std::time::{SystemTime, Duration, Instant};
use std::sync::{Arc, Mutex};
use super::sync::*;

use std::fs::{File, OpenOptions};
use super::state_machine::StateMachine;
use rand::RngCore;



/// At some random time in this range of milliseconds, a follower will become a candidate if no 
const ELECTION_TIMEOUT: (u64, u64) = (400, 800); 

/// If the leader doesn't send anything else within this amount of time, then it will send an empty heartbeat to all followers (this default value would mean around 5 heartbeats each second)
const HEARTBEAT_TIMEOUT: u64 = 200;

/// After this amount of time, we will assume that 
/// 
/// NOTE: This value doesn't matter very much, but the important part is that every single request must have some timeout associated with it to prevent the number of pending incomplete requests from growing indefinately in the case of other servers leaving connections open for an infinite amount of time
const REQUEST_TIMEOUT: u64 = 500;


// TODO: Because the RequestVotes will happen on a fixed interval, we must ensure that 

pub enum Proposal<'a> {
	Pending { term: u64, index: u64 },
	RetryAfter { term: u64, index: u64 },
	NotLeader { leader_hint: Option<&'a ServerDescriptor> }
}


pub type ConsensusModuleHandle = Arc<Mutex<ConsensusModule>>;

pub struct ConsensusModule {
	meta: Metadata,
	config: Configuration,
	log: Arc<Mutex<LogStore + Send + 'static>>,

	/// Triggered whenever we commit more entries
	/// Should be received by the state machine to apply more entries from the log
	//commit_event: EventSender,

	/// Index of the last log entry known to be commited
	/// NOTE: It is not generally necessary to store this, and can be re-initialized always to at least the index of the last applied entry in config or log
	//commit_index: Option<u64>,

	/// For followers, this is the last time we have received a heartbeat from the leader
	/// For candidates, this is the time at which they started their election
	/// For leaders, this is the last time we sent any rpc to our followers
	last_time: SystemTime,

	// Basically this is the persistent state stuff
	state: ServerState,
	
	/// Trigered whenever the state is changed
	/// Should be received by the cycler to update timeouts for heartbeats/elections
	state_event: EventSender,
}

/*
	In order to make a server, we must at least have a server id 
	- First and for-most, if there already exists a file on disk with metadata, then we should use that
	- Otherwise, we must just block until we have a machine id by some other method
		- If an existing cluster exists, then we will ask it to make a new cluster id
		- Otherwise, the main() script must wait for someone to bootstrap us and give ourselves id 1
*/

/*
log.push(LogEntry {
	index: 1,
	term: 1,
	data: LogEntryData::Config(ConfigChange::AddMember(ServerDescriptor {
		id: 1,
		addr: "127.0.0.1:4000".to_string()
	}))
});

ServerState::Leader(ServerLeaderState::new())

*/

impl ConsensusModule {

	// Generally we will need to have a configuration available and such
	// If this machine does not have a machine id, then one must be created before starting the server (either by a bootstrap process or by obtaining a new id from an existing cluster)
	// TODO: Possibly the better option would be to pass in the channel for the state listening (and make the events distinctly typed so that they can't be mixed and matched in the arguments )
	pub fn new(id: ServerId) -> (ConsensusModule, EventReceiver) {

		let log = Arc::new(Mutex::new(MemoryLogStore::new()));

		let state = ServerState::Follower(ServerFollowerState {
			election_timeout: Self::new_election_timeout(),
			last_leader_id: None
		});

		let mut config = Configuration {
			last_applied: 0, // TODO: Convert to an Option
			members: HashSet::new(),
			pending_members: HashSet::new(),
			learners: HashSet::new()
		};

		config.members.insert(ServerDescriptor {
			id: 1,
			addr: "http://127.0.0.1:4001".to_string()
		});

		config.members.insert(ServerDescriptor {
			id: 2,
			addr: "http://127.0.0.1:4002".to_string()
		});

		let (tx, rx) = event();

		// For now this is for a single server with well known id (but no servers in the cluster)
		(ConsensusModule {
			meta: Metadata {
				server_id: id,

				current_term: 0,
				voted_for: None,

				// NOTE: Could be volatile but preserved when posible to make recoveries more robust
				commit_index: 0
			},
			config,

			log,

			last_time: SystemTime::now(),

			state,
			state_event: tx

		}, rx)
	}

	pub fn start(inst: Arc<Mutex<ConsensusModule>>, event: EventReceiver) -> impl Future<Item=(), Error=()> + Send + 'static {

		let id = inst.lock().expect("Failed to lock instance").meta.server_id;
		let service = rpc::run_server(4000 + (id as u16), inst.clone());

		// General loop for managing the server and maintaining leadership, etc.
		
		let cycler = loop_fn((inst, event), |(inst, event)| {

			// TODO: Switch to an Instant and use this one time for this entire loop for everything
			let now = SystemTime::now();

			let mut wait_time = Duration::from_millis(0);
			{

				let mut server = inst.lock().expect("Failed to lock instance");

				// TODO: Ideally the cycler should a time as input
				let dur = server.cycle(inst.clone(), &now);

				// TODO: Should be switched to a tokio::timer which doesn't block anything
				if let Some(d) = dur {
					wait_time = d;
				}
			}

			// If sleep is required, we should run a conditional variable that wakes us up an recycles as needed

			//if false {
			//	return ok(Loop::Break(()));
			//}

			// TODO: If not necessary, we should be able to support zero-wait cycles
			event.wait(wait_time).map(move |event| {
				Loop::Continue((inst, event))
			})
		})
		.map_err(|_| {
			// XXX: I think there is a stray timeout error that could occur here
			()
		});

		// TODO: Finally if possible we should attempt to broadcast our ip address to other servers so they can rediscover us

		service
		.join(cycler)
		.map(|_: ((), ())| ()).map_err(|_| ())
	}

	/*
	/// Propose a new state machine command given some data packet
	pub fn propose_command<'a>(&'a mut self, data: Vec<u8>) -> Proposal<'a> {
		

	}
	*/

	/*
		For the first node, we can unilaterally run a propose_config with
		
		ConfigChange::AddMember({ id: 1, addr: '127.0.0.1:4000' })
			- Naturally to append to ourselves

	*/

	/*
	pub fn propose_config(&mut self, change: ConfigChange) -> Proposal {
		if let ServerState::Leader(_) = self.state {

		}


		// Otherwise, we must 

	}
	*/

	/*
	fn propose_entry(&mut self, data: LogEntryData) -> Proposal {
		if let ServerState::Leader(_) = self.state {
			// Considering we are a leader, that we can know that we definately have a valid term is set

			let index = match self.log.last() { Some(e) => e.term, None => 1 };
			let term = self.meta.current_term;

			self.log.push(LogEntry {
				term,
				index,
				data
			});

			// Now notify everyone of this change

			Proposal::Pending { term, index }
		}
		// Otherwie only 
		else if let ServerState::Follower(s) = self.state {
			// TODO:
			Proposal::NotLeader { leader_hint: Some(self.config.members.get(&0).unwrap()) }
		}
		else {
			Proposal::NotLeader { leader_hint: None }
		}
	}
	*/

	// NOTE: Because most types are private, we probably only want to expose being able to 


	/// Assuming that we are the only server in the cluster, this will unilaterally add itself to the configuration and thus cause it to become the active leader of its one node cluster
	pub fn bootstrap() {

	}

	fn cycle(&mut self, inst_handle: ConsensusModuleHandle, now: &SystemTime) -> Option<Duration> {
		let elapsed = now.duration_since(self.last_time).unwrap_or(Duration::from_millis(0));

		// TODO: If there are no members in the cluster, then this trivially has nothing to do until we get added to someone's cluster or someone will bootstrap us

		match self.state.clone() {
			ServerState::Follower(s) => {
				if elapsed >= s.election_timeout {
					// Needs to 
					self.start_election(inst_handle.clone());					
				}
				else {
					// Otherwise sleep until the next election
					// The preferred method here will be to wait on the conditional variable if 
					// We will probably generalize to passing around Arc<Server> with the 
					return Some(s.election_timeout - elapsed);
				}
			},
			ServerState::Candidate(ref s) => {
				// TODO: This will basically end up being the same exact precedure as for folloders
				// Possibly some logic for retring requests

				if elapsed >= s.election_timeout {
					self.start_election(inst_handle.clone());
				}
				else {
					return Some(s.election_timeout - elapsed);
				}

			},
			ServerState::Leader(ref s) => {

				if elapsed >= Duration::from_millis(HEARTBEAT_TIMEOUT) {
					self.last_time = now.clone();

					println!("Leader performing heartbeat");

					// TOOD: THis will now need to become much better
					let req = AppendEntriesRequest {
						term: self.meta.current_term,
						leader_id: self.meta.server_id,
						prev_log_index: 0,
						prev_log_term: 0,
						entries: vec![],
						leader_commit: 0
					};

					let arr = self.config.members.iter()
					.filter(|s| {
						s.id != self.meta.server_id
					})
					.map(|s| {
						rpc::call_append_entries(s, &req)
						.map(|resp| {

							// TODO: append_entries_callback

							()	
						})
					}).collect::<Vec<_>>();

					let f = join_all(arr)
					.map(|_| ())
					.map_err(|e| {
						eprintln!("{:?}", e);
					});

					tokio::spawn(f);
				}


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
	fn replicate_entries(entries: &[LogEntry]) {

		// This should be invoked once the leader wants to dispatch an AppendEntries requests to its followers to keep them synchronized

		///////

		// Fail if we are not the leader
		// NOTE: If we have failed to heartbeat enough machines recently, then we are no longer a leader

		// Append to local log
		// Send to all other machines

		// Once responses come from a minimum of the nodes, it is successful

		// Increment the commited index

		// Replicate 

		// Generally 

		// But yes, this basically just appends to the local log and then the cycle process could perform the actual replication

		// A client just needs to wait for the commited_index to go higher than the one it sent
		// ^ if the leader changes, it can verify which have been appended based on the term and indexes


	}


	fn start_election(&mut self, inst_handle: ConsensusModuleHandle) {

		// Basically must be run and presisted
		// TODO: If we are retrying because no one responded at any of our messages, then we probably don't need to increment the term again

		// NOTE: Only need to increment if we are not a candidate already or we observed some other server with our current term
		self.meta.current_term += 1;
		self.meta.voted_for = Some(self.meta.server_id);

		// Mark the time at which we started this election
		self.last_time = SystemTime::now();

		println!("Starting election for term: {}", self.meta.current_term);

		// Really not much point to generalizing it mainly because it still requires that we have much more stuff
		self.state = ServerState::Candidate(ServerCandidateState {
			election_timeout: Self::new_election_timeout(),
			votes_received: HashSet::new()
		});


		// We will have an election result encapsulated in the leader

		// Send up a bunch of RPCSs

		let req = RequestVoteRequest {
			term: self.meta.current_term,
			candidate_id: self.meta.server_id,

			// TODO: Grab from the log entries (indexes starting at 1)
			last_log_index: 0, // self.log.len() as u64,
			last_log_term: 0 // self.log.last().unwrap().term
		};

		let sent = self.config.members.iter().filter(|s| {
			s.id != self.meta.server_id
		})
		.map(|s| {

			// NOTE: We should be able to handle individual but literally as soon as we hit a majority we can respond to the request
			// In other cases, we should still maintain a casual timeout

			let id = s.id;

			let inst_handle = inst_handle.clone();

			rpc::call_request_vote(s, &req)
			.and_then(move |resp| {
				// TODO: Only count votes if we haven't yet transitioned yet since the time we started the vote

				let mut inst = inst_handle.lock().expect("Failed to lock instance");
				inst.request_vote_callback(id, resp);		

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

	/// Makes this server a follower in the current term
	fn become_follower(&mut self) {
		self.state = ServerState::Follower(ServerFollowerState {
			election_timeout: Self::new_election_timeout(),
			last_leader_id: None
		});

		self.last_time = SystemTime::now();

		self.state_event.notify();
	}

	/// Run every single time a term index is seen in a remote request or response
	/// If another server has a higher term than us, then we must become a follower
	fn observe_term(&mut self, term: u64) {
		if term > self.meta.current_term {
			self.meta.current_term = term;
			self.meta.voted_for = None;
			self.become_follower();
		}
	}

	/// Handles the response to a RequestVote that this module issued the given server id
	/// This depends on the 
	fn request_vote_callback(&mut self, from_id: ServerId, resp: RequestVoteResponse) {

		self.observe_term(resp.term);


		// All of this only matters if we are the candidate in the current term
		if self.meta.current_term != resp.term {
			return;
		}

		// TODO: Ensure we ignore self votes

		// TODO: It doesn't really help us if we are cloning the object (better to make the state an immutable arc?)
		if let ServerState::Candidate(ref mut s) = self.state.clone() {
			if resp.vote_granted {
				// TODO: This doesn't really work as we are cloning the state 
				s.votes_received.insert(from_id);
			}
			
			let majority = (self.config.members.len() / 2) + 1;

			// If we are still a candidate, then we should have voted for ourselves
			let count = 1 + s.votes_received.len();

			if count >= majority {
				self.state = ServerState::Leader(ServerLeaderState::new());

				// NOTE: Because the heartbeat time is smaller than the election time, the next cycle should force us into doing a heartbeat as the leader

				self.state_event.notify();
			}
		}
	}

	fn append_entries_callback(&mut self, from_id: ServerId, resp: AppendEntriesResponse) {

		self.observe_term(resp.term);

		// Generally if we are still the leader in the given term, then we should be updating our definition for the client

		// Step one is to check if we 

	}


	fn new_election_timeout() -> Duration {
		let mut rng = rand::thread_rng();
		let time = ELECTION_TIMEOUT.0 +
			((rng.next_u32() as u64) * (ELECTION_TIMEOUT.1 - ELECTION_TIMEOUT.0)) / (std::u32::MAX as u64);

		Duration::from_millis(time)
	}


	// Now must get a little bit more serious about it assuming that we have a valid set of commands that we should be sending over

}


impl rpc::ServerService for ConsensusModule {

	/// Called when another server is requesting that we vote for it 
	fn request_vote(&mut self, req: RequestVoteRequest) -> Result<RequestVoteResponse> {

		println!("Received request_vote from {}", req.candidate_id);

		self.observe_term(req.term);

		let granted = {
			if req.term < self.meta.current_term {
				false
			}
			// In this case, the terms must be equal
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

			println!("Casted vote for: {}", req.candidate_id);
		}

		// XXX: Persist to storage before responding

		// NOTE: Much simpler for term to start at 0 right?
		Ok(RequestVoteResponse {
			term: self.meta.current_term,
			vote_granted: granted
		})
	}
	
	// TODO: If we really wanted to, we could have the leader also execute this in order to get consistent local behavior
	fn append_entries(&mut self, req: AppendEntriesRequest) -> Result<AppendEntriesResponse> {

		self.observe_term(req.term);

		// If a candidate observes another leader for the current term, then it should become a follower
		// This is generally triggered by the initial heartbeat that a leader does upon being elected to assert its authority and prevent further elections
		if req.term == self.meta.current_term {
			// TODO: This this should definitely not require a clone
			if let ServerState::Candidate(s) = self.state.clone() {
				self.become_follower();
			}
		}

		let current_term = self.meta.current_term;

		let response = |success: bool| {
			Ok(AppendEntriesResponse {
				term: current_term,
				success,
				last_log_index: 0 // TODO: This is tricky as in some error cases, this shouldprobably be a different value than the most recent one in our logs
			})
		};


		if req.term < self.meta.current_term {
			return response(false);
		}

		// Trivial considering observe_term gurantees the > case
		assert_eq!(req.term, self.meta.current_term);

		match self.state {
			// This is generally the only state we expect to see
			ServerState::Follower(_) => {
				// Update the time now that we have seen a request from the leader in the current term
				self.last_time = SystemTime::now();
				// TODO: Update the leader hint (maybe also update with the ip address we are getting this from)
			},
			// We will only see this when the leader is applying a change to itself
			ServerState::Leader(_) => {
				if req.leader_id != self.meta.server_id {
					return Err("This should never happen. We are receiving append entries from another leader in the same term".into());
				}
			},
			// We should never see this
			ServerState::Candidate(_) => {
				return Err("How can we still be a candidate right now?".into());
			}
		};


		// Sanity checking the request
		if req.entries.len() >= 1 {
			// Sanity check 1: First entry must be immediately after the previous one
			let first = &req.entries[0];
			if first.term < req.prev_log_term || first.index != req.prev_log_index + 1 {
				return Err("Received previous entry does not follow".into());
			}

			// Sanity check 2: All entries must be in sorted order and immediately after one another (because the truncation below depends on them being sorted, this must hold)
			for i in 0..(req.entries.len() - 1) {
				let cur = &req.entries[i];
				let next = &req.entries[i + 1];

				if cur.term > next.term || next.index != cur.index + 1 {
					return Err("Received entries are unsorted, duplicates, or inconsistent".into());
				}
			}
		}
		


		let mut log = self.log.lock().unwrap();

		match log.get_term_at(req.prev_log_index) {
			Some(term) => {
				if term != req.prev_log_term {
					// TODO: If this happens, I need to be able to immediately decrement the leaders index in order to have it resend this record immediately
					return response(false)
				}
			},
			None => return response(false)
		};

		// Index into the entries array of the first new entry not already in our log
		// (this will also be the current index in the below loop)
		let mut first_new = 0;

		for e in req.entries.iter() {
			let existing_term = log.get_term_at(e.index);
			if let Some(t) = existing_term {
				if t == e.term {
					// Entry is already in the log
					first_new += 1;
				}
				else {
					// TODO: If we ever observe attempt to truncate entries that are already locally applied or commited, then we should panic

					// Log is inconsistent
					log.truncate_suffix(e.index); // Should truncate every entry including and after e.index
					break;
				}
			}
			else {
				// Nothing exists at this index, so it is trivially a new index
				break;
			}
		}

		// Assertion: the first new entry we are adding should immediately follow the last index in the our local log as of now
		// TODO: Realistically we should be moving this check close to the append implementation
		// Generally this should never happen considering all of the other checks that we have above
		if first_new < req.entries.len() {
			let last = log.last_entry_index().unwrap_or(LogEntryIndex { index: 0, term: 0 });
			let next = &req.entries[first_new];

			if next.index != last.index + 1 || next.term < last.term {
				return Err("Next new entry is not immediately after our last local one".into());
			}
		}


		// TODO: This could be zero which would be annoying
		let mut last_new = req.prev_log_index;

		// Finally it is time to append some entries
		if req.entries.len() - first_new > 0 {
			// In most cases, we can just take ownership of the whole entries array, otherwise we need to make a new slice of only part of it
			let new_entries = if first_new == 0 {
				req.entries
			} else {
				let mut arr = vec![];
				arr.extend_from_slice(&req.entries[first_new..]);
				arr
			};

			last_new = new_entries.last().unwrap().index;

			log.append(new_entries);
		}


		// NOTE: It is very important that we use the index of the last entry in the request (and not the index of the last entry in our log as we have not necessarily validated up to that far in case the term or leader changed)
		if req.leader_commit > self.meta.commit_index {
			self.meta.commit_index = std::cmp::min(req.leader_commit, last_new);
			// TODO: If changed, trigger event listeners to be notified
		}


		response(true)
	}

}

