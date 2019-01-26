
use super::errors::*;
use super::protos::*;
use super::constraint::*;
use super::consensus::*;
use super::rpc;
use super::sync::*;
use super::atomic::*;
use super::server_protos::*;
use super::log::*;
use bytes::Bytes;
use std::time::Instant;
use futures::future::*;
use futures::prelude::*;
use futures::{Future, Stream};

use std::collections::{HashMap};
use std::time::{Duration};
use std::sync::{Arc, Mutex, MutexGuard};

use tokio::prelude::FutureExt;
use super::state_machine::StateMachine;

/// After this amount of time, we will assume that an rpc request has failed
/// 
/// NOTE: This value doesn't matter very much, but the important part is that every single request must have some timeout associated with it to prevent the number of pending incomplete requests from growing indefinately in the case of other servers leaving connections open for an infinite amount of time (so that we never run out of file descriptors)
const REQUEST_TIMEOUT: u64 = 500;

// Basically whenever we connect to another node with a fresh connection, we must be able to negogiate with each the correct pair of cluster id and server ids on both ends otherwise we are connecting to the wrong server/cluster and that would be problematic (especially when it comes to aoiding duplicate votes because of duplicate connections)


/*
	Maintaining route information
	- Every server knows its own initial address
	- When another server calls us, it should always tell it what its address is
	
	Updating known peers
	- Starting at startup, a server will gossip about its id and addr to other servers 
	- A server could maintain a list of all server ids we have ever seen
	- If we have not recently told a server about our address, then we will go ahead and tell it

	Discovering unknown peers
	- In local networks, use udp broadcast to broadcast our identity
		- Other servers will discover us by listening for this (and they they will recipricate the action using the updating known operation)
	- In test environments, use a static list of hosts
	- In managed environments enable querying of a 

	- If at least one server (other than ourselves) is well known by one of the above methods
		-> Ask it for a list of all servers that it knows about
		-> This would function as a CockroachDB/MongoDB style discovery process given at least one server already in the server

*/

/*
	Further improvements:
	- compared to etcd/raft
		- Making into a pure state machine
			- All outputs of the state machine are currently exposed and consumed in our finish_tick function in addition to a separate response message which is given as a direct return value to functions invoked on the ConsensusModule for RPC calls
		- Separating out the StateMachine
			- the etcd Node class currently does not have the responsibility of writing to the state machine

	- TODO: In the case that our log or snapshot gets corrupted, we want some integrated way to automatically repair from another node without having to do a full clean erase and reapply 
		- NOTE: Because this may destroy our quorum, we may want to allow configuring for a minimum of quorum + 1 or something like that for new changes
			- Or enforce a durability level for old and aged entries
*/

// TODO: Would also be nice to have some warning of when a disk operation is required to read back an entry as this is generally a failure on our part


/// Represents everything needed to start up a Server object
pub struct ServerInitialState {
	/// Value of the metadata initially
	pub meta: ServerMetadata,
	
	/// A way to persist the metadata
	pub meta_file: BlobFile,

	/// Snapshot of the configuration to use
	pub config_snapshot: ServerConfigurationSnapshot,

	/// A way to persist the configuration snapshot
	pub config_file: BlobFile,

	/// The initial or restored log
	/// NOTE: The server takes ownership of the log
	pub log: Box<LogStorage + Send + Sync + 'static>,
	
	/// Instantiated instance of the state machine
	/// (either an initial empty one or one restored from a local snapshot)
	pub state_machine: Arc<StateMachine + Send + Sync + 'static>,
	
	/// Index of the last log entry applied to the state machine given
	/// Should be 0 unless this is a state machine that was recovered from a snapshot
	pub last_applied: u64,
}


/// Represents a single node of the cluster
/// Internally this manages the log, rpcs, and applying changes to the 
pub struct Server {
	shared: Arc<ServerShared>,
}

/// Server variables that can be shared by many different threads
struct ServerShared {

	state: Mutex<ServerState>,

	// TODO: Need not have a lock for this right? as it is not mutable
	// Definately we want to lock the LogStorage separately from the rest of this code
	log: Arc<LogStorage + Send + Sync + 'static>,

	state_machine: Arc<StateMachine + Send + Sync + 'static>,

	/// Holds the index of the log index most recently persisted to disk
	/// This is eventually consistent with the index in the log itself
	/// NOTE: This is safe to always have a term for as it should always be in the log
	match_index: Condvar<LogPosition, LogPosition>,

	/// Holds the value of the current commit_index for the server
	/// This is eventually consistent with the index in the internal consensus module
	/// NOTE: This is the highest commit index currently available in the log and not the highest index ever seen
	/// A listener will be notified if we got a commit_index at least as up to date as their given position
	/// NOTE: The state machine will listen for (0,0) always so that it is always sent new entries to apply
	/// XXX: This is not guranteed to have a well known term unless we start recording the commit_term in the metadata for the initial value
	commit_index: Condvar<LogPosition, LogPosition>,

	/// Last log index applied to the state machine
	/// This should only ever be modified by the separate applier
	last_applied: Condvar<u64, u64>,
}

/// All the mutable state for the server that you hold a lock in order to look at
struct ServerState {
	inst: ConsensusModule,

	// TODO: Move those out
	meta_file: BlobFile, config_file: BlobFile,

	// XXX: Contain client connections
	// XXX: No point in these being locked
	// XXX: Another event will be used to send to the thread that flushes the log
	// It will emit back through the match_index log
	
	/// Trigered whenever the state or configuration is changed
	/// TODO: currently this will not fire on configuration changes
	/// Should be received by the cycler to update timeouts for heartbeats/elections
	/// TODO: The events don't need a lock (but if we are locking, then we might as well use it right)
	state_changed: ChangeSender, state_receiver: Option<ChangeReceiver>,

	/// Triggered whenever a new entry has been queued onto the log
	/// Used to trigger the log to get flushed to persistent storage
	log_changed: ChangeSender, log_receiver: Option<ChangeReceiver>,

	// For each observed server id, this is the last known address at which it can be found
	// This is used 
	// XXX: Cleaning up old routes: Everything in the cluster can always be durable for a long time
	// Otherwise, we will maintain up to 16 unused addresses to be pushed out on an LRU basis
	// because servers are only ever added one and a time and configurations should get synced quickly this should always be reasonable
	// The only complication would be new servers which won't have the entire config yet
	// We could either never delete servers with ids smaller than our lates log index and/or make sure that servers are always started with a complete configuration snaphot (which includes a snaphot of the config ips + routing information)
	// TODO: Should we make these under a different lock so that we can process messages while running the state forward (especially as sending a response requires no locking)
	routes: HashMap<ServerId, String>
}

impl Server {

	pub fn new(
		initial: ServerInitialState,
	) -> Self {

		let ServerInitialState {
			mut meta, meta_file,
			config_snapshot, config_file,
			log,
			state_machine,
			last_applied
		} = initial;

		let log: Arc<LogStorage + Send + Sync + 'static> = Arc::from(log);

		// We make no assumption that the commit_index is consistently persisted, and if it isn't we can initialize to the the last_applied of the state machine as we will never apply an uncomitted change to the state machine
		// NOTE: THe ConsensusModule similarly performs this check on the config snapshot
		if last_applied > meta.meta.commit_index {
			meta.meta.commit_index = last_applied;
		}

		// Gurantee no log discontinuities (only overlaps are allowed)
		// This is similar to the check on the config snapshot that we do in the consensus module
		if last_applied + 1 < log.first_index().unwrap_or(0) {
			panic!("State machine snapshot is from before the start of the log");
		}

		// TODO: If all persisted snapshots contain more entries than the log, then we can trivially schedule a log prefix compaction 

		if meta.meta.commit_index > log.last_index().unwrap_or(0) {
			// This may occur on a leader that has not matched itself yet
		}


		let inst = ConsensusModule::new(meta.id, meta.meta, config_snapshot.config, log.clone());

		let (tx_state, rx_state) = change();
		let (tx_log, rx_log) = change();

		let mut routes = HashMap::new();

		routes.insert(1, "http://127.0.0.1:4001".to_string());
		routes.insert(2, "http://127.0.0.1:4002".to_string());

		let state = ServerState {
			inst,
			meta_file, config_file,
			state_changed: tx_state, state_receiver: Some(rx_state),
			log_changed: tx_log, log_receiver: Some(rx_log),
			routes
		};

		let shared = ServerShared {
			state: Mutex::new(state),
			log,
			state_machine,

			// NOTE: these will be initialized below
			match_index: Condvar::new(LogPosition { index: 0, term: 0 }),
			commit_index: Condvar::new(LogPosition { index: 0, term: 0 }),

			last_applied: Condvar::new(last_applied)
		};

		shared.update_match_index();
		ServerShared::update_commit_index(&shared, &shared.state.lock().unwrap());

		Server {
			shared: Arc::new(shared)
		}
	}

	// THis will need to be the thing running a tick method which must just block for events

	// NOTE: If we also give it a state machine, we can do that for people too
	pub fn start(server: Arc<Server>) -> impl Future<Item=(), Error=()> + Send + 'static {

		let (id, state_changed, log_changed) = {
			let mut state = server.shared.state.lock().expect("Failed to lock instance");

			(
				state.inst.id(),

				// If these errors out, then it means that we tried to start the server more than once
				state.state_receiver.take().expect("State receiver already taken"),
				state.log_receiver.take().expect("Log receiver already taken")
			)
		};

		let service = rpc::run_server::<Arc<Server>, Server>(4000 + (id as u16), server.clone());

		let cycler = Self::run_cycler(&server, state_changed);
		let matcher = Self::run_matcher(&server, log_changed);
		let applier = Self::run_applier(&server);

		// TODO: Finally if possible we should attempt to broadcast our ip address to other servers so they can rediscover us

		service
		// NOTE: Because in bootstrap mode a server can spawn requests immediately without the first futures cycle, it may spawn stuff before tokio is ready, so we must make this lazy
		.join4(
			lazy(|| cycler),
			lazy(|| matcher),
			lazy(|| applier)
		)
		.map(|_| ()).map_err(|_| ())
	}

	/// Runs the idle loop for managing the server and maintaining leadership, etc. in the case that no other events occur to drive the server
	fn run_cycler(
		server_handle: &Arc<Server>,
		state_changed: ChangeReceiver
	) -> impl Future<Item=(), Error=()> + Send + 'static {

		let shared_handle = server_handle.shared.clone();
		
		loop_fn((shared_handle, state_changed), |(shared_handle, state_changed)| {

			let tick_time: Instant;
			let mut wait_time = Duration::from_millis(0);

			{
				let mut state = shared_handle.state.lock().expect("Failed to lock server instance");

				let mut tick = Tick::empty();
				state.inst.cycle(&mut tick);

				tick_time = tick.time.clone();

				// NOTE: We take it so that the finish_tick doesn't re-trigger this loop and prevent sleeping all together
				if let Some(d) = tick.next_tick.take() {
					wait_time = d;
				}
				else {
					// TODO: Ideally refactor to represent always having a next time as part of every operation
					eprintln!("Server cycled with no next tick time");
				}

				// Now tick must have a reference to the 
				// TODO: Ideally have this run without the need for the state to be held
				ServerShared::finish_tick(&shared_handle, state, tick);
			}

			println!("Sleep {:?}", wait_time);

			// TODO: If not necessary, we should be able to support zero-wait cycles (although now those should never happen as the consensus module should internally now always converge in one run)
			state_changed.wait(tick_time + wait_time).map(move |state_changed| {
				Loop::Continue((shared_handle, state_changed))
			})
		})
		.map_err(|_| {
			// XXX: I think there is a stray timeout error that could occur here
			()
		})
	}

	/// Flushes log entries to persistent storage as they come in
	/// This is responsible for pushing changes to the match_index variable
	fn run_matcher(
		server: &Arc<Server>,
		log_changed: ChangeReceiver
	) -> impl Future<Item=(), Error=()> + Send + 'static {

		// TODO: Must explicitly run in a separate thread until we can make disk flushing a non-blocking operation

		let shared = server.shared.clone();

		loop_fn((shared, log_changed), |(shared, log_changed)| {

			// NOTE: The log object is responsible for doing its own internal locking as needed
			// TODO: Should we make this non-blocking right now	
			shared.log.flush();

			// TODO: Ideally if the log requires a lock, this should use the same lock used for updating this as well (or the match_index should be returned from the flush method <- Preferably also with the term that was flushed)
			shared.update_match_index();

			// TODO: There is generally no good reason to wake up for any timeout so we will just wait for a really long time
			let next_time = Instant::now() + Duration::from_secs(10);
			
			log_changed.wait(next_time).map(move |log_changed| {
				Loop::Continue((shared, log_changed))
			})
		})
	}

	/// When entries are comitted, this will apply them to the state machine
	/// This is the exclusive modifier of the last_applied shared variable and is also responsible for triggerring snapshots on the state machine when we want one to happen
	/// NOTE: If this thing fails, we can still participate in raft but we can not perform snapshots or handle read/write queries 
	fn run_applier(
		server: &Arc<Server>
	) -> impl Future<Item=(), Error=()> + Send + 'static {

		loop_fn(server.shared.clone(), |shared| {

			let commit_index = shared.commit_index.lock().index;
			let mut last_applied = *shared.last_applied.lock();
			
			let state_machine = &shared.state_machine;

			// Apply all committed entries to state machine
			while last_applied < commit_index {
				let entry = shared.log.entry(last_applied + 1);
				if let Some(e) = entry {
					
					if let LogEntryData::Command(ref data) = e.data {
						// TODO: This may error out (in which case we have a really big problem as we can't progress )
						// TODO:
						state_machine.apply(data);
					}

					last_applied += 1;
				}
				else {
					// Our log may be behind the commit_index in the consensus module, but the commit_index conditional variable should always be at most at the newest value in our log
					eprintln!("Need to apply an entry not in our log yet");
					break;
				}
			}

			// Update last_applied
			{
				let mut guard = shared.last_applied.lock();
				if last_applied > *guard {
					*guard = last_applied;
					guard.notify_all();
				}
			}

			// Wait for the next time commit_index changes 
			{
				let guard = shared.commit_index.lock();

				// If the commit index changed since last we checked, we can immediately cycle again
				if guard.index != commit_index {
					// We can immediately cycle again
					return Either::A(ok(Loop::Continue(shared.clone())));
				}

				let shared2 = shared.clone();

				// Otherwise we will wait for it to change
				Either::B(
					guard.wait(LogPosition { term: 0, index: 0 })
					.then(move |_| {
						ok(Loop::Continue(shared2))
					})
				)
			}
		})
	}


}


impl ServerShared {


	// TODO: If this fails, we may need to stop the server
	// NOTE: This function assumes that the given state guard is for the exact same state as represented within this shared state
	pub fn finish_tick<'a>(shared: &Arc<Self>, state: MutexGuard<'a, ServerState>, tick: Tick) -> Result<()> {

		// If new entries were appended, we must notify the flusher
		if tick.new_entries {

			// When our log has fewer entries than are committed, the commit index may go up
			// TODO: Will end up being a redundant operation with the below one
			Self::update_commit_index(shared, &state);

			// XXX: Simple scenario is to just use the fact that we have the lock
			state.log_changed.notify();
		}

		// XXX: Single sender for just the 
		// XXX: If we batch together two redundant RequestVote requests, the tick produced by the second one will not require a metadata change
		// ^ The issue with this is that we can't just respond with the second message unless the previous metadata that required a flush from the first request is flushed
		// ^ This is why it would be useful to have monotonic demands on this
		if tick.meta {
			// TODO: Potentially batchable if we choose to make this something that can do an async write to the disk
			state.meta_file.store(&rpc::marshal(ServerMetadataRef {
				id: state.inst.id(),
				cluster_id: 0,
				meta: state.inst.meta()
			})?)?;

			Self::update_commit_index(shared, &state);
		}

		// TODO: In most cases it is not necessary to persist the config unless we are doing a compaction, but we should schedule a task to ensurethat this gets saved eventually
		if tick.config {
			state.config_file.store(&rpc::marshal(ServerConfigurationSnapshotRef {
				config: state.inst.config_snapshot(),
				routes: &state.routes
			})?)?;
		}


		Self::dispatch_messages(&shared, &state, tick.messages);

		// TODO: Verify this encapsulates all cases of meaningful state changes
		// TODO: Verify that in most cases this is never actually triggerred as it isn't all that gast
		if tick.next_tick.is_some() {
			// TODO: The real question is whether or not to use the fake that we have a lock to make this more efficient than using an atomic 
			state.state_changed.notify();
		}

		Ok(())
	}

	fn update_match_index(&self) {
		// Getting latest match_index
		let cur_mi = self.log.match_index().unwrap_or(0);
		let cur_mt = self.log.term(cur_mi).unwrap();
		let cur = LogPosition {
			index: cur_mi,
			term: cur_mt
		};

		// Updating it
		let mut mi = self.match_index.lock();
		// NOTE: The match_index is not necessarily monotonic in the case of log truncations
		if *mi != cur {
			*mi = cur;

			mi.notify_all();

			
			// For leaders, an update to the match_index may advance the commit_index, so we will trigger a cycle of the state
			// NOTE: Typically this will not help as we will usually get AppendEntries requests back before the local flush is complete but this will help with the single-node case
			self.state.lock().unwrap().state_changed.notify();
		}
	}


	/// Notifies anyone waiting on something to get committed
	/// TODO: Realistically as long as we enforce that it atomically goes up, we don't need to have a lock on the state in order to perform this update
	fn update_commit_index(shared: &ServerShared, state: &ServerState) {

		let latest_commit_index = state.inst.meta().commit_index;

		let latest = match shared.log.term(latest_commit_index) {
			// If the commited index is in the log, use it
			Some(term) => {
				LogPosition {
					index: latest_commit_index,
					term
				}
			},
			// Otherwise, more data has been comitted than is in our log, so we will only mark up to the last entry in our lag
			None => {
				let last_log_index = shared.log.last_index().unwrap_or(0);
				let last_log_term = shared.log.term(last_log_index).unwrap();

				LogPosition {
					index: last_log_index,
					term: last_log_term
				}
			}
		};


		let mut ci = shared.commit_index.lock();

		// NOTE '<' should be sufficent here as the commit index should never go backwards
		if *ci != latest {
			*ci = latest;
			ci.notify_all();
		}
	}

	fn dispatch_messages(shared: &Arc<Self>, state: &ServerState, messages: Vec<Message>) {
	
		println!("Send {}", messages.len());

		if messages.len() == 0 {
			return;
		}

		// Noteably we will basically have two sets of 

		let mut append_entries = vec![];
		let mut request_votes = vec![];

		// TODO: We should chain on some promise holding one side of a channel so that we can cancel this entire request later if we end up needing to 
		let new_request_vote = |
			to_id: ServerId, addr: &String, req: &RequestVoteRequest
		| {
			let shared = shared.clone();

			rpc::call_request_vote(addr, req)
			.timeout(Duration::from_millis(REQUEST_TIMEOUT))
			.then(move |res| -> FutureResult<(), ()> {
				
				let mut state = shared.state.lock().expect("Failed to lock instance");
				let mut tick = Tick::empty();

				if let Ok(resp) = res {
					state.inst.request_vote_callback(to_id, resp, &mut tick);
				}

				Self::finish_tick(&shared, state, tick);

				ok(())
			})

		};

		let new_append_entries = |
			to_id: ServerId, addr: &String, req: &AppendEntriesRequest, last_log_index: u64
		| {

			let shared = shared.clone();

			let ret = rpc::call_append_entries(addr, req)
			.timeout(Duration::from_millis(REQUEST_TIMEOUT))
			.then(move |res| -> FutureResult<(), ()> {

				let mut state = shared.state.lock().unwrap();
				let mut tick = Tick::empty();

				if let Ok(resp) = res {
					// NOTE: Here we assume that this request send everything up to and including last_log_index
					// ^ Alternatively, we could have just looked at the request object that we have in order to determine this
					state.inst.append_entries_callback(to_id, last_log_index, resp, &mut tick);
				}
				else {
					state.inst.append_entries_noresponse(to_id, &mut tick);
				}

				Self::finish_tick(&shared, state, tick);
			
				ok(())
			});
			// TODO: In the case of a timeout or other error, we would still like to unblock this server from having a pending_request

			ret
		};

		for msg in messages {
			for to_id in msg.to {

				// XXX: Assumes well known routes
				// ^ if this results in a miss, then this is a good insective to ask some other server for a new list of server addrs immediately
				let addr = state.routes.get(&to_id).unwrap();

				match msg.body {
					MessageBody::AppendEntries(ref req, ref last_log_index) => {
						append_entries.push(new_append_entries(to_id, addr, req, *last_log_index));
					},
					MessageBody::RequestVote(ref req) => {
						request_votes.push(new_request_vote(to_id, addr, req));
					},
					_ => {}
					// TODO: Handle all cases
				};
			}
		}


		// Let them all loose
		let f = join_all(append_entries).join(join_all(request_votes))
		.map(|_| ())
		.map_err(|_| {
			//eprintln!("{:?}", e);
			()
		});

		tokio::spawn(f);
	}


	// TODO: Can we more generically implement as waiting on a Constraint driven by a Condition which can block for a specific value
	// TODO: Cleanup and try to deduplicate with Proposal polling
	pub fn wait_for_match<T: 'static>(shared: Arc<ServerShared>, c: MatchConstraint<T>)
		-> impl Future<Item=T, Error=Error> + Send where T: Send
	{
		loop_fn((shared, c), |(shared, c)| {
			match c.poll() {
				ConstraintPoll::Satisfied(v) => return Either::A(ok(Loop::Break(v))),
				ConstraintPoll::Unsatisfiable => return Either::A(err("Halted progress on getting match".into())),
				ConstraintPoll::Pending((c, pos)) => {
					let fut = {
						let mi = shared.match_index.lock();
						mi.wait(pos)
					};
					
					Either::B(fut.then(move |_| {
						ok(Loop::Continue((shared, c)))
					}))
				}
			}
		})
	}

	/// TODO: We must also be careful about when the commit index
	/// Waits for some conclusion on a log entry pending committment
	/// This can either be from it getting comitted or from it becomming never comitted
	/// A resolution occurs once a higher log index is comitted or a higher term is comitted
	pub fn wait_for_commit(shared: Arc<ServerShared>, pos: LogPosition)
		-> impl Future<Item=(), Error=Error> + Send
	{

		loop_fn((shared, pos), |(shared, pos)| {

			// TODO: The ideal case is to the make the condition variables value sufficiently reliable that we don't need to ever lock the server state in order to check this condition
			// But yes, both commit_index and commit_term can be atomic variables
			// No need to lock then 

			let (ci, ct) = {
				let state = shared.state.lock().unwrap();
				let ci = state.inst.meta().commit_index;
				let ct = shared.log.term(ci).unwrap();
				(ci, ct)
			};

			if ct > pos.term || ci >= pos.index {
				Either::A(ok(Loop::Break(())))
			}
			else {
				let shared2 = shared.clone();
				let lk = shared2.commit_index.lock();

				// TODO: commit_index can be implemented using two atomic integers denoting the term and ...
				Either::B(lk.wait(LogPosition { term: 0, index: 0 }).then(move |_| {
					ok(Loop::Continue((shared, pos)))
				}))
			}
		})

		// TODO: Will we ever get a request to truncate the log without an actual committment? (either way it isn't binding to the future of this proposal until it actually comitted something that is in conflict with us)
	}

	/// Given a known to be comitted index, this waits until it is available in the state machine
	/// NOTE: You should always first wait for an item to be comitted before waiting for it to get applied (otherwise if the leader gets demoted, then the wrong position may get applied)
	pub fn wait_for_applied(shared: Arc<ServerShared>, pos: LogPosition) -> impl Future<Item=(), Error=Error> + Send {

		loop_fn((shared, pos), |(shared, pos)| {

			let guard = shared.last_applied.lock();
			if *guard >= pos.index {
				return Either::A(ok( Loop::Break(()) ));
			}

			let shared2 = shared.clone();
			Either::B(guard.wait(pos.index).then(move |_| {
				ok(Loop::Continue((shared2, pos)))
			}))
		})

	}


}

impl rpc::ServerService for Server {

	fn pre_vote(&self, req: RequestVoteRequest) -> rpc::ServiceFuture<RequestVoteResponse> { to_future_box!({
		let mut state = self.shared.state.lock().unwrap();
		
		// NOTE: Tick must be created after the state is locked to gurantee monotonic time always
		let mut tick = Tick::empty();
		let res = state.inst.pre_vote(req, &mut tick);

		// Hopefully no messages were produced, we may only have anew hard state, but this is no immediate need to definitely flush it

		ServerShared::finish_tick(&self.shared, state, tick)?;

		Ok(res)
	}) }

	fn request_vote(&self, req: RequestVoteRequest) -> rpc::ServiceFuture<RequestVoteResponse> { to_future_box!({
		let mut state = self.shared.state.lock().unwrap();

		let mut tick = Tick::empty();
		let res = state.inst.request_vote(req, &mut tick);

		ServerShared::finish_tick(&self.shared, state, tick)?;

		Ok(res.persisted())
	})}
	
	fn append_entries(
		&self, req: AppendEntriesRequest
	) -> rpc::ServiceFuture<AppendEntriesResponse> {
		
		// TODO: In the case that entries are immediately written, this is overly expensive

		Box::new(to_future!({

			let mut state = self.shared.state.lock().unwrap();

			let mut tick = Tick::empty();
			let res = state.inst.append_entries(req, &mut tick)?;

			ServerShared::finish_tick(&self.shared, state, tick)?;

			Ok((self.shared.clone(), res))

		}).and_then(|(shared, res)| {
			ServerShared::wait_for_match(shared, res)			
		}))
	}
	
	fn timeout_now(&self, req: TimeoutNow) -> rpc::ServiceFuture<()> { to_future_box!({

		let mut state = self.shared.state.lock().unwrap();

		let mut tick = Tick::empty();
		state.inst.timeout_now(req, &mut tick)?;

		ServerShared::finish_tick(&self.shared, state, tick)?;

		Ok(())

	}) }

	// TODO: This may become a ClientService method only? (although it is still sufficiently internal that we don't want just any old client to be using this)
	fn propose(&self, req: ProposeRequest) -> rpc::ServiceFuture<ProposeResponse> {

		Box::new(to_future!({
			let mut state = self.shared.state.lock().unwrap();

			let mut tick = Tick::empty();
			let res = state.inst.propose_entry(req.data, &mut tick);

			ServerShared::finish_tick(&self.shared, state, tick)?;

			if let ProposeResult::Started(prop) = res {
				Ok((req.wait, self.shared.clone(), prop))
			}
			else {
				println!("propose result: {:?}", res);
				Err("Not implemented".into())
			}
		}).and_then(|(should_wait, shared, prop)| {
			
			if !should_wait {
				return Either::A(ok(ProposeResponse {
					term: prop.term,
					index: prop.index
				}));
			}

			// TODO: Must ensure that wait_for_commit responses immediately if it is already comitted
			Either::B(ServerShared::wait_for_commit(shared.clone(), prop.clone())
			.and_then(move |_| {

				let state = shared.state.lock().unwrap();
				let res = state.inst.proposal_status(&prop);

				match res {
					ProposalStatus::Commited => ok(ProposeResponse {
						term: prop.term,
						index: prop.index
					}),
					ProposalStatus::Failed => err("Proposal failed".into()),
					_ => {
						println!("GOT BACK {:?}", res);

						err("Proposal indeterminant".into())
					}
				}
			}))
		}))
	}

}
