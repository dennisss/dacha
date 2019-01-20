
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
use tokio::prelude::FutureExt;

use super::state_machine::StateMachine;
use rand::RngCore;
use std::borrow::Borrow;


// NOTE: Blocking on a proposal to get some conclusion will be the role of blocking on a one-shot based in some external code
// But most read requests will adictionally want to block on the state machine being fully commited up to some minimum index (I say minimum for the case of point-in-time transactions that don't care about newer stuff)

/// At some random time in this range of milliseconds, a follower will become a candidate if no 
const ELECTION_TIMEOUT: (u64, u64) = (400, 800); 

/// If the leader doesn't send anything else within this amount of time, then it will send an empty heartbeat to all followers (this default value would mean around 5 heartbeats each second)
const HEARTBEAT_TIMEOUT: u64 = 200;

/// After this amount of time, we will assume that 
/// 
/// NOTE: This value doesn't matter very much, but the important part is that every single request must have some timeout associated with it to prevent the number of pending incomplete requests from growing indefinately in the case of other servers leaving connections open for an infinite amount of time (so that we never run out of file descriptors)
const REQUEST_TIMEOUT: u64 = 500;


// TODO: Because the RequestVotes will happen on a fixed interval, we must ensure that 

// NOTE: This is basically the same type as a LogEntryIndex (we might as well wrap a LogEntryIndex and make the contents of a proposal opaque to other programs using the consensus api)
#[derive(Debug)]
pub struct Proposal {
	pub term: u64,
	pub index: u64
}

#[derive(Debug)]
pub enum ProposeResult<'a> {
	Pending(Proposal),
	RetryAfter(Proposal),
	NotLeader { leader_hint: Option<&'a ServerDescriptor> }
}

pub enum ProposalStatus {

	/// The proposal has been safely replicated and should get applied to the state machine soon
	Commited,

	/// The proposal is still pending replication
	Pending,

	/// We don't know anything about this proposal (at least right now)
	/// This should only happen if a proposal was made on the leader but the status was checked on a follower
	Missing,
	
	/// The proposal has been abandoned and will never be commited
	/// Typically this means that another leader took over before the entry was fully replicated
	Failed,

	/// Implies that the status is permanently unavailable meaning that the proposal is from before the start of the raft log (only in the snapshot or no where at all)
	Unavailable
}


pub type ConsensusModuleHandle = Arc<Mutex<ConsensusModule>>;


#[derive(Clone)]
struct ConfigurationPending {
	/// Index of the last entry in our log that changes the config
	pub last_change: u64,

	/// Configuration as it was before the last change
	pub previous: Configuration
}


/*
	Store methods:
	- WriteMetadata
	- WriteConfig
	- AppendEntries

	- Everything else is non-important

*/

pub struct ConsensusModule {
	/// Id of the current server we are representing
	id: ServerId,

	meta: Metadata,

	/// The currently active configuration of the cluster
	config: Configuration,

	/// If the current configuration is not yet commited, then this will mark the last change available
	config_pending: Option<ConfigurationPending>,

	log: Arc<Mutex<LogStore + Send + 'static>>,

	/// Triggered whenever we commit more entries
	/// Should be received by the state machine to apply more entries from the log
	commit_event: EventSender,

	/// Index of the last log entry known to be commited
	/// NOTE: It is not generally necessary to store this, and can be re-initialized always to at least the index of the last applied entry in config or log
	//commit_index: Option<u64>,

	/// For followers, this is the last time we have received a heartbeat from the leader
	/// For candidates, this is the time at which they started their election
	/// For leaders, this is the last time we sent any rpc to our followers
	//last_time: Instant,

	// Basically this is the persistent state stuff
	state: ServerState,
	
	/// Trigered whenever the state or configuration is changed
	/// Should be received by the cycler to update timeouts for heartbeats/elections
	state_event: EventSender
}

/*
	In order to make a server, we must at least have a server id 
	- First and for-most, if there already exists a file on disk with metadata, then we should use that
	- Otherwise, we must just block until we have a machine id by some other method
		- If an existing cluster exists, then we will ask it to make a new cluster id
		- Otherwise, the main() script must wait for someone to bootstrap us and give ourselves id 1
*/


impl ConsensusModule {

	// Generally we will need to have a configuration available and such
	// If this machine does not have a machine id, then one must be created before starting the server (either by a bootstrap process or by obtaining a new id from an existing cluster)
	// TODO: Possibly the better option would be to pass in the channel for the state listening (and make the events distinctly typed so that they can't be mixed and matched in the arguments )
	pub fn new(id: ServerId, meta: Metadata, mut config: Configuration, log: Arc<Mutex<LogStore + Send + 'static>>)
		-> (ConsensusModule, EventReceiver, EventReceiver) {

		// TODO: If we have a reference to the state machine, then we can use it to determine another boundary of the min commit index

		// We should have never saved an uncommitted config to storage. Uncommitted configurations should only exist in memory
		if config.last_applied > meta.commit_index {
			panic!("Config snapshot is ahead of the commit index")
		}

		let mut config_pending = None;

		// If the log contains more entries than the config, advance the config forward
		{
			let log = log.lock().unwrap();
			let last_log_index = log.last_index().unwrap_or(0);

			while config.last_applied < last_log_index {
				let e = log.entry(config.last_applied + 1).unwrap();

				if let LogEntryData::Config(ref change) = e.data {
					if e.index < meta.commit_index {
						config_pending = Some(ConfigurationPending {
							last_change: e.index,
							previous: config.clone()
						});
					}

					config.apply(e.index, change.clone());
				}
				else {
					config.last_applied = e.index;
				}
			}

		}


		let state = ServerState::Follower(ServerFollowerState {
			election_timeout: Self::new_election_timeout(),
			last_leader_id: None,
			last_heartbeat: Instant::now()
		});


		let (tx, rx) = event();

		let (tx_commit, rx_commit) = event();

		// XXX: Assert that we never safe a configuration to disk or initialize with a config that has uncommited entries in it

		// For now this is for a single server with well known id (but no servers in the cluster)
		(ConsensusModule {
			id,
			meta,
			config,
			config_pending,

			log,

			state,
			state_event: tx,

			commit_event: tx_commit

		}, rx, rx_commit)
	}

	pub fn start(inst: Arc<Mutex<ConsensusModule>>, event: EventReceiver) -> impl Future<Item=(), Error=()> + Send + 'static {

		let id = inst.lock().expect("Failed to lock instance").id;
		let service = rpc::run_server(4000 + (id as u16), inst.clone());

		// General loop for managing the server and maintaining leadership, etc.
		
		// NOTE: Because in bootstrap mode a server can spawn requests immediately without the first futures cycle, it may spawn stuff before tokio is ready, so we must make this lazy
		let cycler = lazy(|| loop_fn((inst, event), |(inst, event)| {

			// TODO: Switch to an Instant and use this one time for this entire loop for everything
			let now = Instant::now();

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
		}));

		// TODO: Finally if possible we should attempt to broadcast our ip address to other servers so they can rediscover us

		service
		.join(cycler)
		.map(|_: ((), ())| ()).map_err(|_| ())
	}

	/// Propose a new state machine command given some data packet
	pub fn propose_command(&mut self, data: Vec<u8>) -> ProposeResult {
		self.propose_entry(LogEntryData::Command(data))
	}

	pub fn propose_noop(&mut self) -> ProposeResult {
		self.propose_entry(LogEntryData::Noop)
	}

	// How this will work, in general, wait for an AddServer RPC, 
	/*
	pub fn propose_config(&mut self, change: ConfigChange) -> Proposal {
		if let ServerState::Leader(_) = self.state {

		}


		// Otherwise, we must 

	}
	*/


	/*
	/// Checks the progress of a previously iniated proposal
	/// This can be safely queried on any server in the cluster but naturally the status on the current leader will be the first to converge
	pub fn proposal_status(&self, prop: &Proposal) -> ProposalStatus {
		if self.meta.commit_index >= prop.index {
			let log = self.log.lock().unwrap();
			log.get_term_at
		}
	}
	*/


	fn propose_entry(&mut self, data: LogEntryData) -> ProposeResult {
		if let ServerState::Leader(_) = self.state {

			let mut log = self.log.lock().unwrap();

			let index = log.last_index().unwrap_or(0) + 1;
			let term = self.meta.current_term;

			// Considering we are a leader, this should always true, as we only ever start elections at 1
			assert!(term > 0);


			// If the new proposal is for a config change, block it until 
			// TODO: Realistically we actually just need to check against the current commit index for doing this (as that may be higher)
			let mut config_change = None;
			if let LogEntryData::Config(ref change) = data {
				if let Some(ref pending) = self.config_pending {
					return ProposeResult::RetryAfter(Proposal {
						index: pending.last_change,
						term: log.term(pending.last_change).unwrap()
					});
				}

				config_change = Some(change.clone());
			}

			// XXX: 

			log.append(&vec![LogEntry {
				term,
				index,
				data
			}]);

			// As soon as the configuratio change gets into the log, apply it
			if let Some(change) = config_change {
				self.config_pending = Some(ConfigurationPending {
					last_change: index,
					previous: self.config.clone()
				});

				self.config.apply(index, change);
			}

			// Next time the cycling thread is availabe, it should replicate these entries
			self.state_event.notify();

			ProposeResult::Pending(Proposal { term, index })
		}
		// Otherwie only 
		else if let ServerState::Follower(ref s) = self.state {
			// TODO:
			ProposeResult::NotLeader { leader_hint: Some(self.config.members.get(&0).unwrap()) }
		}
		else {
			ProposeResult::NotLeader { leader_hint: None }
		}
	}

	// NOTE: Because most types are private, we probably only want to expose being able to 


	/// Assuming that we are the only server in the cluster, this will unilaterally add itself to the configuration and thus cause it to become the active leader of its one node cluster
	pub fn bootstrap() {

	}

	// Input (meta, config, state) -> (meta, state)   * config does not get changed
	// May produce messages and new log entries
	fn cycle(&mut self, inst_handle: ConsensusModuleHandle, now: &Instant) -> Option<Duration> {

		// If there are no members n the cluster, there is trivially nothing to do, so we might as well wait indefinitely
		// If we didn't have this line, then the follower code would go wild trying to propose an election
		// Additionally there is no work to be done if we are not in the voting members
		// TODO: We should assert that a non-voting member never starts an election and other servers should never note for a non-voting member
		if self.config.members.len() == 0 || self.config.members.get(&self.id).is_none() {
			return Some(Duration::from_secs(1));
		}


		// TODO: If there are no members in the cluster, then this trivially has nothing to do until we get added to someone's cluster or someone will bootstrap us

		match self.state.clone() {
			ServerState::Follower(s) => {
				let elapsed = now.duration_since(s.last_heartbeat);

				// NOTE: If we are the only server in the cluster, then we can trivially win the election without waiting
				if elapsed >= s.election_timeout || self.config.members.len() == 1 {
					// Needs to 
					self.start_election(inst_handle.clone(), now);					
				}
				else {
					// Otherwise sleep until the next election
					// The preferred method here will be to wait on the conditional variable if 
					// We will probably generalize to passing around Arc<Server> with the 
					return Some(s.election_timeout - elapsed);
				}
			},
			ServerState::Candidate(ref s) => {

				let majority = self.majority_size();

				// If we are still a candidate, then we should have voted for ourselves
				// TODO: Count 1 only if we are in the current voting configuration?
				let count = 1 + s.votes_received.len();

				if count >= majority {
					// XXX: must acquire the log right here
					
					// TODO: For a single-node system, this should occur instantly without any timeouts
					println!("Woohoo! we are now the leader");

					let last_log_index = {
						let log = self.log.lock().unwrap();
						log.last_index().unwrap_or(0)
					};


					let servers = self.config.iter()
						.filter(|s| s.id != self.id)
						.map(|s| {
							(s.id, ServerProgress::new(last_log_index))
						})
						.collect::<_>();

					self.state = ServerState::Leader(ServerLeaderState {
						servers
					});

					// We are starting our leadership term with at least one uncomitted entry from a pervious term. To immediately commit it, we will propose a no-op
					if self.meta.commit_index < last_log_index {
						self.propose_noop();
					}

					// On the next cycle we will be a leader

					self.state_event.notify();

					return None;
				}
				else {

					let elapsed = now.duration_since(s.election_start);

					// TODO: This will basically end up being the same exact precedure as for folloders
					// Possibly some logic for retring requests during the same election cycle

					if elapsed >= s.election_timeout {
						self.start_election(inst_handle.clone(), now);
					}
					else {
						return Some(s.election_timeout - elapsed);
					}
				}
			},
			ServerState::Leader(ref s) => {
				
				/*
					The final major thing for leaders is ensuring that their list of server progresses are well up to date

					- If we have a unified place to apply config changes, then this would be trivial

					^ THis must also happen whenever basically anything changes, (so probably easier to insert and update things as needed)
					- Immutable relative to config. Internal mutable on one part of it

					- So if we have a configuration change, then we must insert or delete an entry from the list 
				*/

				if let Some(next_commit_index) = self.find_next_commit_index(&s) {
					println!("Commiting up to: {}", next_commit_index);
					self.update_commited(next_commit_index)
				}

				self.replicate_entries(inst_handle.clone(), now);

				// TODO: respond with a good timeout?

				/*

				// TODO: If the last entry is not commited and from a previous term, create a heartbeat that also proposes a Noop entry so that we can commit everything
					// Probably better to propose that upon the first transition to becoming a leader
				*/

				let mut next_heartbeat = Duration::from_millis(HEARTBEAT_TIMEOUT);

				/*
				// TODO: This will not use the same data as changed in replicate_entries because we copied it
				for (id, s) in s.servers.iter() {

				}
				*/

				// TODO: We could be more specific about this by getting the shortest amount of time after the last heartbeat we've send out up to now (replicate_entries could probably give us this )
				return Some(next_heartbeat);
			}
		};

		None
	}


	/*
		- Another consideration would be to maintain exactly once semantics when we are sneding a command to a server

		In LogCabin, applying entries is essentially secondary to consensus module
			- The state machine simply asynchronously applies entries eventually once the consensus module has accepted them

			- So yes, general idea is to decouple the consensus module from the log and from the state machine
	*/


	/// On the leader, this will find the best value for the next commit index if any is currently possible 
	fn find_next_commit_index(&self, s: &ServerLeaderState) -> Option<u64> {

		let log = self.log.lock().unwrap();

		// Starting at the last entry in our log, go backwards until we can find an entry that we can mark as commited
		// TODO: ci can also more specifically start at the max value across all match_indexes
		let mut ci = log.last_index().unwrap_or(0);

		let majority = self.majority_size();
		while ci > self.meta.commit_index {

			let term = log.term(ci).unwrap();

			if term < self.meta.current_term {
				// Because terms are monotonic, if we get to an entry that is < our current term, we will never see any more entries at our current term
				break
			}
			else if term == self.meta.current_term {

				// Count how many other voting members have successfully commited the 
				let mut count = 1; // < Because we are the leader, we count as one voting member
				for (id, e) in s.servers.iter() {
					// Skip non-voting members or ourselves
					if !self.config.members.contains(id) || *id == self.id {
						continue;
					}

					if e.match_index >= ci {
						count += 1;
					}
				}

				if count >= majority {
					return Some(ci);
				}
			}

			// Try the previous entry next time
			ci -= 1;
		}

		None
	}


	/// TODO: In the case of many servers in the cluster, enforce some maximum limit on requests going out of this server at any one time and prioritize members that are actually part of the voting process

	// NOTE: If we have failed to heartbeat enough machines recently, then we are no longer a leader

	/// On the leader, this will produce requests to replicate or maintain the state of the log on all other servers in this cluster
	/// This also handles sending out heartbeats as a base case of that process 
	fn replicate_entries<'a>(&'a mut self, inst_handle: ConsensusModuleHandle, now: &Instant) {

		let state: &'a mut ServerLeaderState = match self.state {
			ServerState::Leader(ref mut s) => s,

			// Generally this entire function should only be called if we are a leader, so hopefully this never happen
			_ => panic!("Not the leader")
		};

		let config = &self.config;

		let leader_id = self.id;
		let term = self.meta.current_term;
		let leader_commit = self.meta.commit_index;


		let log = self.log.lock().unwrap();

		let last_log_index = log.last_index().unwrap_or(0);
		//let last_log_term = log.term(last_log_index).unwrap();

		// Given some previous index, produces a request containing all entries after that index
		// TODO: Long term this could reuse the same request objects as we will typically be sending the same request over and over again
		let new_request = |prev_log_index: u64| -> AppendEntriesRequest {
			
			let mut entries = vec![];
			for i in (prev_log_index + 1)..(last_log_index + 1) {
				entries.push( log.entry(i).unwrap().clone() );
			}

			AppendEntriesRequest {
				term,
				leader_id,
				prev_log_index,
				prev_log_term: log.term(prev_log_index).unwrap(),
				entries,
				leader_commit
			}
		};


		let arr = config.iter()
		.filter_map(|server| {
			
			// Don't send to ourselves (the leader)
			if server.id == leader_id {
				return None;
			}

			// Make sure there is a progress entry for the this server
			// TODO: Currently no mechanism for removing servers from the leaders state if they are removed from this (TODO: Eventually we should get rid of the insert here and make sure that we always rely on the config changes for this)
			let progress = {
				if !state.servers.contains_key(&server.id) {
					state.servers.insert(server.id, ServerProgress::new(last_log_index));
				}

				state.servers.get_mut(&server.id).unwrap()
			};
			

			// Ignore servers we are currently sending something to
			if progress.request_pending {
				return None;
			}

			// If this server is already up-to-date, don't replicate if the last request was within the heartbeat timeout
			if progress.match_index >= last_log_index {
				if let Some(ref time) = progress.last_sent {
					// TODO: This version of duration_since may panic
					if now.duration_since(*time) < Duration::from_millis(HEARTBEAT_TIMEOUT) {
						return None;
					}
				}
			}


			// Otherwise, we are definately going to make a request to it

			progress.request_pending = true;
			progress.last_sent = Some(now.clone());

			let inst_handle = inst_handle.clone();

			let req = new_request(progress.next_index - 1);

			// Mainly so that we don't need a reference to progress descriptor anymore
			let id = server.id;

			// TODO: We want to handle errors and timeout the request to be able to reset back the clock
			let ret = rpc::call_append_entries(server, &req)
			.timeout(Duration::from_millis(REQUEST_TIMEOUT))
			.then(move |res| -> FutureResult<(), ()> {

				let mut inst = inst_handle.lock().unwrap();

				if let Ok(resp) = res {
					// NOTE: Here we assume that this request send everything up to and including last_log_index
					inst.append_entries_callback(id, last_log_index, resp);
				}
				else {
					inst.append_entries_noresponse(id);
				}
			
				ok(())
			});
			// TODO: In the case of a timeout or other error, we would still like to unblock this server from having a pending_request

			Some(ret)
		})
		.collect::<Vec<_>>();


		// Let them all loose
		let f = join_all(arr)
		.map(|_| ())
		.map_err(|_| {
			//eprintln!("{:?}", e);
			()
		});

		tokio::spawn(f);
	}

	// Mutates (meta, state) in place
	fn start_election(&mut self, inst_handle: ConsensusModuleHandle, now: &Instant) {

		// Unless we are an active candidate who has already voted for themselves in the current term and we we haven't received conflicting responses, we must increment the term counter for this election
		let must_increment = {
			if let ServerState::Candidate(ref s) = self.state {
				if !s.some_rejected {
					false
				}
				else { true }
			}
			else { true }
		};

		if must_increment {
			self.meta.current_term += 1;
			self.meta.voted_for = Some(self.id);
		}


		println!("Starting election for term: {}", self.meta.current_term);

		// Really not much point to generalizing it mainly because it still requires that we have much more stuff
		self.state = ServerState::Candidate(ServerCandidateState {
			election_start: now.clone(),
			election_timeout: Self::new_election_timeout(),
			votes_received: HashSet::new(),
			some_rejected: false
		});

		self.perform_election(inst_handle);
	}

	fn perform_election(&self, inst_handle: ConsensusModuleHandle) {

		let (last_log_index, last_log_term) = {
			let log = self.log.lock().unwrap();

			let idx = log.last_index().unwrap_or(0);
			let term = log.term(idx).unwrap();

			(idx, term)
		};

		let req = RequestVoteRequest {
			term: self.meta.current_term,
			candidate_id: self.id,
			last_log_index,
			last_log_term
		};

		let sent = self.config.members.iter().filter(|s| {
			s.id != self.id
		})
		.map(|s| {

			let id = s.id;
			let inst_handle = inst_handle.clone();

			rpc::call_request_vote(s, &req)
			.timeout(Duration::from_millis(REQUEST_TIMEOUT))
			.and_then(move |resp| {
				let mut inst = inst_handle.lock().expect("Failed to lock instance");
				inst.request_vote_callback(id, resp);
				ok(())
			})

		}).collect::<Vec<_>>();


		let f = join_all(sent)
		.map(|_| ())
		.map_err(|e| {
			eprintln!("Error while requesting votes: {:?}", e);
			()
		});

		// TODO: We should chain on some promise holding one side of a channel so that we can cancel this entire request later if we end up needing to 

		tokio::spawn(f);
	}

	/// Makes this server a follower in the current term
	fn become_follower(&mut self) {
		self.state = ServerState::Follower(ServerFollowerState {
			election_timeout: Self::new_election_timeout(),
			last_leader_id: None,
			last_heartbeat: Instant::now()
		});

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

	/// Run this whenever the commited index should be changed
	fn update_commited(&mut self, index: u64) {
		self.meta.commit_index = index;

		// TODO: We should save the meta to disk right here (but we don't want to over-save it like in the case of the code above us observing a new term and performing another write)
		// Noteably we can always derive the newest term from the last entry in the log max'ed with the hard state

		if let Some(pending) = self.config_pending.clone() {
			if pending.last_change <= self.meta.commit_index {
				self.config_pending = None;

				// XXX: Right here we are able to store the config to disk 
			}
		}

		// TODO: Only if it actually changed
		self.commit_event.notify();
	}

	/// Number of votes for voting members required to get anything done
	/// NOTE: This is always at least one, so a cluster of zero members should require at least 1 vote
	fn majority_size(&self) -> usize {
		// A safe-guard for empty clusters. Because our implementation rightn ow always counts one vote from ourselves, we will just make sure that a majority in a zero node cluster is near impossible instead of just requiring 1 vote
		if self.config.members.len() == 0 {
			return std::usize::MAX;
		}

		(self.config.members.len() / 2) + 1
	}

	/// Handles the response to a RequestVote that this module issued the given server id
	/// This depends on the 
	fn request_vote_callback(&mut self, from_id: ServerId, resp: RequestVoteResponse) {

		self.observe_term(resp.term);

		// All of this only matters if we are still the candidate in the current term
		// (aka the state hasn't changed since we initially requested a vote)
		if self.meta.current_term != resp.term {
			return;
		}

		// This should generally never happen
		if from_id == self.id {
			eprintln!("Rejected duplicate self vote?");
			return;
		}

		if let ServerState::Candidate(ref mut s) = self.state {
			if resp.vote_granted {
				s.votes_received.insert(from_id);
			}
			else {
				s.some_rejected = true;
			}
			
			// NOTE: Only really needed if we just achieved a majority
			self.state_event.notify();
		}
	}

	// last_index should be the index of the last entry that we sent via this request
	fn append_entries_callback(&mut self, from_id: ServerId, last_index: u64, resp: AppendEntriesResponse) {

		self.observe_term(resp.term);

		if let ServerState::Leader(ref mut s) = self.state {
			// TODO: Across multiple election cycles, this may no longer be available
			let mut progress = s.servers.get_mut(&from_id).unwrap();

			if resp.success { // On success, we should 
				if last_index > progress.match_index { // NOTE: THis condition should only be needed if we allow multiple concurrent requests to occur
					progress.match_index = last_index;
					progress.next_index = last_index + 1;
				}
			}
			else {
				// Meaning that we must role back the log index
				// TODO: Assert that next_index becomes strictly smaller

				if let Some(idx) = resp.last_log_index {
					progress.next_index = idx + 1;
				}
				else {
					// TODO: Integer overflow
					progress.next_index -= 1;
				}
			}

			progress.request_pending = false;

			// In case something above was mutated, we will notify the cycler to trigger any additional requests to be dispatched
			self.state_event.notify();
		}
	}

	/// Handles the event of received no response or an error/timeout from an append_entries request
	fn append_entries_noresponse(&mut self, from_id: ServerId) {
		if let ServerState::Leader(ref mut s) = self.state {
			let mut progress = s.servers.get_mut(&from_id).unwrap();
			progress.request_pending = false;
		}
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

		let should_grant = |this: &Self| {

			// NOTE: Accordingly with the last part of Section 4.1 in the Raft thesis, a server should grant votes to servers not currently in their configuration in order to gurantee availability during member additions

			if req.term < this.meta.current_term {
				return false;
			}

			// In this case, the terms must be equal
				
			let (last_log_index, last_log_term) = {
				let log = this.log.lock().unwrap();
				let idx = log.last_index().unwrap_or(0);
				let term = log.term(idx).unwrap();
				(idx, term)
			};

			// Whether or not the candidate's log is at least as 'up-to-date' as our own log
			let up_to_date = {
				// If the terms differ, the candidate must have a higher log term
				req.last_log_term > last_log_term ||

				// If the terms are equal, the index of the entry must be at least as far along as ours
				(req.last_log_term == last_log_term && req.last_log_index >= last_log_index)
			};

			if !up_to_date {
				return false;
			}

			match this.meta.voted_for {
				// If we have already voted in this term, we are not allowed to change our minds
				Some(id) => {
					id == req.candidate_id
				},
				// Grant the vote if we have not yet voted
				None => true
			}
		};

		let granted = should_grant(self);

		if granted {
			self.meta.voted_for = Some(req.candidate_id);
			println!("Casted vote for: {}", req.candidate_id);
		}

		// XXX: Persist to storage before responding

		Ok(RequestVoteResponse {
			term: self.meta.current_term,
			vote_granted: granted
		})
	}
	
	// TODO: Another very important thing to have is a more generic gossip protocol for updateing server configurations so that restarts don't take the whole server down due to misconfigured addresses

	// TODO: If we really wanted to, we could have the leader also execute this in order to get consistent local behavior
	
	fn append_entries(&mut self, req: AppendEntriesRequest) -> Result<AppendEntriesResponse> {

		// NOTE: It is totally normal for this to receive a request from a server that does not exist in our configuration as we may be in the middle of a configuration change adn this could be the request that adds that server to the configuration

		self.observe_term(req.term);

		// If a candidate observes another leader for the current term, then it should become a follower
		// This is generally triggered by the initial heartbeat that a leader does upon being elected to assert its authority and prevent further elections
		if req.term == self.meta.current_term {
			
			let is_candidate = match self.state { ServerState::Candidate(_) => true, _ => false };

			if is_candidate {
				self.become_follower();
			}
		}

		let current_term = self.meta.current_term;

		let response = |success: bool, last_log_index: Option<u64>| {
			Ok(AppendEntriesResponse {
				term: current_term,
				success,
				last_log_index // TODO: This is tricky as in some error cases, this shouldprobably be a different value than the most recent one in our logs
			})
		};


		if req.term < self.meta.current_term {
			// In this case, this is not the current leader so we will reject them
			// This rejection will give the server a higher term index and thus it will demote itself
			return response(false, None);
		}

		// Trivial considering observe_term gurantees the > case
		assert_eq!(req.term, self.meta.current_term);

		match self.state {
			// This is generally the only state we expect to see
			ServerState::Follower(ref mut s) => {
				// Update the time now that we have seen a request from the leader in the current term
				s.last_heartbeat = Instant::now();
				s.last_leader_id = Some(req.leader_id);
			},
			// We will only see this when the leader is applying a change to itself
			ServerState::Leader(_) => {
				// NOTE: In all cases, we currently don't use this track for anything
				if req.leader_id != self.id {
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
		


		// TODO: The clone is mainly so that we can get around other mutation rules below
		let log_handle = self.log.clone();
		let mut log = log_handle.lock().unwrap();


		// This should never happen as the snapshot should only contain comitted entries which should never be resent
		if req.prev_log_index < log.first_index().unwrap_or(1) - 1 {
			return Err("Requested previous log entry is before the start of the log".into());
		}

		match log.term(req.prev_log_index) {
			Some(term) => {
				if term != req.prev_log_term {
					// In this case, our log contains an entry that conflicts with the leader and we will end up needing to overwrite/truncate at least one entry in order to reach consensus
					// We could respond with an index of None so that the leader tries decrementing one index at a time, but instead, we will ask it to decrement down to our last last known commit point so that all future append_entries requests are guranteed to suceed but may take some time to get to the conflict point
					// TODO: Possibly do some type of binary search (next time try 3/4 of the way to the end of the prev entry from the commit_index)
					return response(false, Some(self.meta.commit_index))
				}
			},
			// In this case, we are receiving changes beyond the end of our log, so we will respond with the last index in our log so that we don't get any sequential requests beyond that point
			None => return response(false, Some(log.last_index().unwrap_or(0)))
		};

		// Index into the entries array of the first new entry not already in our log
		// (this will also be the current index in the below loop)
		let mut first_new = 0;

		for e in req.entries.iter() {
			let existing_term = log.term(e.index);
			if let Some(t) = existing_term {
				if t == e.term {
					// Entry is already in the log
					first_new += 1;
				}
				else {
					// Log is inconsistent: Must roll back all changes in the local log

					if self.meta.commit_index >= e.index {
						return Err("Refusing to truncate changes already locally committed".into());
					}

					// If the current configuration is uncommitted, we need to restore the old one if the last change to it is being removed from the log
					if let Some(ref pending) = self.config_pending.clone() {
						if pending.last_change <= e.index {
							self.config = pending.previous.clone();
							self.config_pending = None;
						}
					}

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

			let last_log_index = log.last_index().unwrap_or(0);
			let last_log_term = log.term(last_log_index).unwrap();

			let next = &req.entries[first_new];

			if next.index != last_log_index + 1 || next.term < last_log_term {
				return Err("Next new entry is not immediately after our last local one".into());
			}
		}


		// TODO: This could be zero which would be annoying
		let mut last_new = req.prev_log_index;

		// Finally it is time to append some entries
		if req.entries.len() - first_new > 0 {

			let new_entries = &req.entries[first_new..];
			last_new = new_entries.last().unwrap().index;

			log.append(new_entries);

			// Immediately incorporate any configuration changes
			for e in new_entries {
				if let LogEntryData::Config(ref change) = e.data {
					self.config_pending = Some(ConfigurationPending {
						last_change: e.index,
						previous: self.config.clone()
					});

					self.config.apply(e.index, change.clone());
				}
			}
		}

		// NOTE: It is very important that we use the index of the last entry in the request (and not the index of the last entry in our log as we have not necessarily validated up to that far in case the term or leader changed)
		if req.leader_commit > self.meta.commit_index {
			let next_commit_index =  std::cmp::min(req.leader_commit, last_new);
			self.update_commited(next_commit_index);

			// TODO: We should save the meta to disk right here (but we don't want to over-save it like in the case of the code above us observing a new term and performing another write)
			// Noteably we can always derive the newest term from the last entry in the log max'ed with the hard state

			if let Some(pending) = self.config_pending.clone() {
				if pending.last_change <= self.meta.commit_index {
					self.config_pending = None;

					// XXX: Right here we are able to store the config to disk 
				}
			}

			// TODO: Only if it actually changed
			self.commit_event.notify();
		}

		// NOTE: We don't need to send the last_log_index in the case of success
		response(true, None)
	}

	fn propose(&mut self, req: ProposeRequest) -> Result<ProposeResponse> {

		let res = self.propose_entry(req.data);

		if let ProposeResult::Pending(prop) = res {
			Ok(ProposeResponse {
				term: prop.term,
				index: prop.index
			})
		}
		else {
			println!("propose result: {:?}", res);
			Err("Not implemented".into())
		}
	}


}

