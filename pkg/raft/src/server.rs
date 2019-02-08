
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
use futures::sync::oneshot;

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
	NOTE: We do need to have a sense of a 

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

/*
	Other scenarios:
	- Ticks may be cumulative
	- AKA use a single tick objectict to accumulate multiple changes to the metadata and to messages that must be sent out
	- With messages, we want some method of telling the ConsensusModule to defer generating messages until everything is said and done (to avoid the situation of creating multiple messages where the initial ones could be just not sent given future information processed by the module)

	- This would require that 
*/

// TODO: Would also be nice to have some warning of when a disk operation is required to read back an entry as this is generally a failure on our part

#[derive(Debug)]
pub enum ExecuteError {
	Propose(ProposeError),
	NoResult,

	/*
		Other errors

	*/

	// Also possibly that it just plain old failed to be committed
}


/// Represents everything needed to start up a Server object
pub struct ServerInitialState<R> {
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
	pub state_machine: Arc<StateMachine<R> + Send + Sync + 'static>,
	
	/// Index of the last log entry applied to the state machine given
	/// Should be 0 unless this is a state machine that was recovered from a snapshot
	pub last_applied: u64,
}


/// Represents a single node of the cluster
/// Internally this manages the log, rpcs, and applying changes to the 
pub struct Server<R> {
	shared: Arc<ServerShared<R>>,
}

/// Server variables that can be shared by many different threads
struct ServerShared<R> {

	state: Mutex<ServerState<R>>,

	/// Used for network message sending and connection management
	client: rpc::Client,

	// TODO: Need not have a lock for this right? as it is not mutable
	// Definately we want to lock the LogStorage separately from the rest of this code
	log: Arc<LogStorage + Send + Sync + 'static>,

	state_machine: Arc<StateMachine<R> + Send + Sync + 'static>,

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
struct ServerState<R> {
	inst: ConsensusModule,

	// TODO: Move those out
	meta_file: BlobFile, config_file: BlobFile,
	
	/// Trigered whenever the state or configuration is changed
	/// TODO: currently this will not fire on configuration changes
	/// Should be received by the cycler to update timeouts for heartbeats/elections
	/// TODO: The events don't need a lock (but if we are locking, then we might as well use it right)
	state_changed: ChangeSender, state_receiver: Option<ChangeReceiver>,

	/// The next time at which a cycle is planned to occur at (used to deduplicate notifying the state_changed event)
	scheduled_cycle: Option<Instant>,

	/// Triggered whenever a new entry has been queued onto the log
	/// Used to trigger the log to get flushed to persistent storage
	log_changed: ChangeSender, log_receiver: Option<ChangeReceiver>,

	/// Whenever an operation is proposed, this will store callbacks that will be given back the result once it is applied
	callbacks: std::collections::LinkedList<(LogPosition, oneshot::Sender<Option<R>>)>
}

impl<R: Send + 'static> Server<R> {

	pub fn new(
		client: rpc::Client,
		initial: ServerInitialState<R>,
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

		// TODO: Routing info now no longer a part of core server responsibilities


		let state = ServerState {
			inst,
			meta_file, config_file,
			state_changed: tx_state, state_receiver: Some(rx_state),
			scheduled_cycle: None,
			log_changed: tx_log, log_receiver: Some(rx_log),
			callbacks: std::collections::LinkedList::new() 
		};

		let shared = Arc::new(ServerShared {
			state: Mutex::new(state),
			client,
			log,
			state_machine,

			// NOTE: these will be initialized below
			match_index: Condvar::new(LogPosition { index: 0, term: 0 }),
			commit_index: Condvar::new(LogPosition { index: 0, term: 0 }),

			last_applied: Condvar::new(last_applied)
		});


		ServerShared::update_match_index(&shared);
		ServerShared::update_commit_index(&shared, &shared.state.lock().unwrap());

		Server {
			shared
		}
	}

	// NOTE: If we also give it a state machine, we can do that for people too
	pub fn start(server: Arc<Self>) -> impl Future<Item=(), Error=()> + Send + 'static {

		let (id, state_changed, log_changed) = {
			let mut state = server.shared.state.lock().expect("Failed to lock instance");

			(
				state.inst.id(),

				// If these errors out, then it means that we tried to start the server more than once
				state.state_receiver.take().expect("State receiver already taken"),
				state.log_receiver.take().expect("Log receiver already taken")
			)
		};

		// Must 
		{
			let agent = server.shared.client.agent().lock().unwrap();
			if let Some(desc) = agent.identity {
				// Already running on some other server?
			}
		}

		/*
			Other concerns:
			- Making sure that the routes data always stays well saved on disk
			- Won't change frequently though

			- We will be making our own identity here though
		*/

		// Likewise need store 

		// Therefore we no longer need a tuple really

		// XXX: Right before starting this, we can completely state the identity of our server 
		// Although we would ideally do this easier (but later should also be fine?)

		let service = rpc::run_server(
			4000 + (id as u16), server.clone(), &rpc::ServerService_router::<Arc<Self>, Self>
		);

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
		server: &Arc<Self>,
		state_changed: ChangeReceiver
	) -> impl Future<Item=(), Error=()> + Send + 'static {

		let shared = server.shared.clone();
		
		loop_fn((shared, state_changed), |(shared, state_changed)| {

			// TODO: For a single node, we should almost never need to cycle
			println!("Run cycler");

			let next_cycle = ServerShared::run_tick(&shared, |state, tick| {

				state.inst.cycle(tick);

				// NOTE: We take it so that the finish_tick doesn't re-trigger this loop and prevent sleeping all together
				if let Some(d) = tick.next_tick.take() {
					let t = tick.time + d;
					state.scheduled_cycle = Some(t.clone());
					t
				}
				else {
					// TODO: Ideally refactor to represent always having a next time as part of every operation
					eprintln!("Server cycled with no next tick time");
					tick.time
				}
			});

			// TODO: Currently issue being that this gets run every single time something gets comitted (even though that usually doesn't really matter)
			// Cycles like this should generally only be for heartbeats or replication events and nothing else
			//println!("Sleep {:?}", wait_time);

			state_changed.wait_until(next_cycle).map(move |state_changed| {
				Loop::Continue((shared, state_changed))
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
		server: &Arc<Self>,
		log_changed: ChangeReceiver
	) -> impl Future<Item=(), Error=()> + Send + 'static {

		// TODO: Must explicitly run in a separate thread until we can make disk flushing a non-blocking operation

		let shared = server.shared.clone();

		// XXX: We can also block once the server is shutting down

		loop_fn((shared, log_changed), |(shared, log_changed)| {

			// NOTE: The log object is responsible for doing its own internal locking as needed
			// TODO: Should we make this non-blocking right now	
			if let Err(e) = shared.log.flush() {
				eprintln!("Matcher failed to flush log: {:?}", e);
				return Either::A(ok(Loop::Break(())));

				// TODO: If something like this fails then we need to make sure that we can reject all requestions instead of stalling them for a match

				// TODO: The other issue is that if the failure is not completely atomic, then the index may have been updated in the log internals incorrectly without the flush following through properly
			}

			// TODO: Ideally if the log requires a lock, this should use the same lock used for updating this as well (or the match_index should be returned from the flush method <- Preferably also with the term that was flushed)
			ServerShared::update_match_index(&shared);

			Either::B(log_changed.wait().map(move |log_changed| {
				Loop::Continue((shared, log_changed))
			}))
		})
	}

	/// When entries are comitted, this will apply them to the state machine
	/// This is the exclusive modifier of the last_applied shared variable and is also responsible for triggerring snapshots on the state machine when we want one to happen
	/// NOTE: If this thing fails, we can still participate in raft but we can not perform snapshots or handle read/write queries 
	fn run_applier(
		server: &Arc<Self>
	) -> impl Future<Item=(), Error=()> + Send + 'static {


		loop_fn((server.shared.clone(), std::collections::LinkedList::new()), |(shared, mut callbacks)| {

			let commit_index = shared.commit_index.lock().index;
			let mut last_applied = *shared.last_applied.lock();
			
			// Take ownership of all pending callbacks (as long as a callback is appended to the list before the commit_index variable is incremented, this should always see them)
			{
				let mut state = shared.state.lock().unwrap();
				callbacks.append(&mut state.callbacks);
			}

			// TODO: Suppose we have the item in our log but it gets truncated, then in this case, callbacks will all be blocked until a new operation of some type is proposed

			{

			let state_machine = &shared.state_machine;

			// Apply all committed entries to state machine
			while last_applied < commit_index {
				let entry = shared.log.entry(last_applied + 1);
				if let Some(e) = entry {
					
					let ret = if let LogEntryData::Command(ref data) = e.data {
						match state_machine.apply(e.index, data) {
							Ok(v) => Some(v),
							Err(e) => {
								// TODO: Ideally notify everyone that all progress has been halted
								// If we are the leader, then we should probably demote ourselves to a healthier node
								eprintln!("Applier failed to apply to state machine: {:?}", e);
								return Either::A(ok(Loop::Break(())));
							}
						}
					} else {
						// Other types of log entries produce no output and generally any callbacks specified shouldn't expect any output
						None
					};

					// TODO: the main complication is that we should probably execute all of the callbacks after we have updated the last_applied index so that they are guranteed to see a consistent view of the world if they try to observe its value

					// So we should probably defer all results until after that 

					// Resolve/reject callbacks waiting for this change to get commited
					// TODO: In general, we should assert that the linked list is monotonically increasing always based on proposal indexes
					// TODO: the other thing is that callbacks can be rejected early in the case of something newer getting commited which would override it
					while callbacks.len() > 0 {
						let first = callbacks.front().unwrap().0.clone();

						if e.term > first.term || e.index >= first.index {
							let item = callbacks.pop_front().unwrap();

							if e.term == first.term && e.index == first.index {
								item.1.send(ret);
								break; // NOTE: This is not really necessary as it should immediately get completed on the next run through the loop by the other break 
							}
							// Otherwise, older than the current entry
							else {
								item.1.send(None);
							}
						}
						// Otherwise possibly more recent than the current commit
						else {
							break;
						}
					}


					last_applied += 1;
				}
				else {
					// Our log may be behind the commit_index in the consensus module, but the commit_index conditional variable should always be at most at the newest value in our log
					// (so if we see this, then we have a bug somewhere in this file)
					eprintln!("Need to apply an entry not in our log yet");
					break;
				}
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
			let waiter = {
				let guard = shared.commit_index.lock();

				// If the commit index changed since last we checked, we can immediately cycle again
				if guard.index != commit_index {
					// We can immediately cycle again
					// TODO: We should be able to refactor out this clone
					return Either::A(ok(Loop::Continue((shared.clone(), callbacks))));
				}

				guard.wait(LogPosition { term: 0, index: 0 })
			};

			// Otherwise we will wait for it to change
			Either::B(
				waiter
				.then(move |_| {
					ok(Loop::Continue((shared, callbacks)))
				})
			)
		})
	}


	// Executing a command remotely from a non-leader
	// -> 'Pause' the throw-away of unused results on the applier
	// -> Instead append them to an internal buffer
	// -> Probably best to assign it a client identifier (The only difference is that this will be a client interface which will asyncronously determine that a change is our own)
	// -> Propose a change
	// -> Hope that we get the response back from propose before we advance the state machine beyond that point (with issue being that we don't know the index until after the propose responds)
	// -> Then use the locally available result to resolve the callback as needed

	/*
		The ordering assertion:
		- given that we receive back the result of AppendEntries before that of 

		- Simple compare and set operation
			- requires having a well structure schema
			- Compare and set is most trivial to do if we have a concept of a key version
			- any change to the key resets it's version
			- Versions are monotonic timestamps associated with the key
				- We will use the index of the entry being applied for this
				- This will allow us to get proper behavior across deletions of a key as those would remove the key properly
				- Future edits would require that the version is <= the read_index used to fetch the deleted key
	*/


	/*
		Upon losing our position as leader, callbacks may still end up being applied
		- But if multiple election timeouts pass without a callback making any progress (aka we are no longer the leader and don't can't communicate with the current leader), then callbacks should be timed out
	*/

	/*
		Maintaining client liveness
		- Registered callback will be canceled after 4 election average election cycles have passed:
			- As a leader, we received a quorum of followers
			- Or we as a follow reached the leader
			- This is to generally meant to cancel all active requests once we lose liveness of the majority of the servers

		- Other callbacks
			- wait_for_match
				- Mainly just needed to know when a response can be sent out to an AppendEntries request
			- wait_for_commit
				- Must be cancelable by the same conditions as the callbacks


		We want a lite-wait way to start up arbitrary commands that don't require a return value from the state machine
			- Also useful for 
	*/


	/// Will propose a new change and will return a future that resolves once it has either suceeded to be executed, or has failed
	/// General failures include: 
	/// - For what ever reason we missed the timeout <- NoResult error
	/// - Not the leader     <- ProposeError
	/// - Commit started but was overriden <- In this case we should (for this we may want ot wait for a commit before )
	/// 
	/// NOTE: In order for this to resolve in all cases, we assume that a leader will always issue a no-op at the start of its term if it notices that it has uncommited entries in its own log or if it notices that another server has uncommited entries in its log
	/// NOTE: If we are the leader and we lose contact with our followers or if we are executing via a connection to a leader that we lose, then we should trigger all pending callbacks to fail because of timeout
	pub fn execute(&self, cmd: Vec<u8>) -> impl Future<Item=R, Error=ExecuteError> + Send {

		let res = ServerShared::run_tick(&self.shared, |state, tick| {
			let r = state.inst.propose_entry(LogEntryData::Command(cmd), tick);

			r.map(|prop| {
				let (tx, rx) = oneshot::channel();
				state.callbacks.push_back((prop, tx));
				rx
			})
		});

		let rx = match res {
			Ok(v) => v,
			Err(e) => return Either::A(err(ExecuteError::Propose(e)))
		};

		Either::B(rx
		.map_err(|e| {
			// TODO: Check what this one is
			eprintln!("RX FAILURE");

			ExecuteError::NoResult
		}) 
		.and_then(|v| {
			match v {
				Some(v) => ok(v),

				// TODO: In this case, we would like to distinguish between an operation that was rejected and one that is known to have properly failed
				// ^ If we don't know if it will ever be applied, then we can retry only idempotent commands without needing to ask the client to retry it's full cycle
				// ^ Otherwise, if it is known to be no where in the log, then we can definitely retry it
				None => err(ExecuteError::NoResult) // < TODO: In this case check what is up in the commit
			}
		}))
	}

	/// Blocks until the state machine can be read such that all changes that were commited before the time at which this was called have been flushed to disk
	/// TODO: Other consistency modes: 
	/// - For follower reads, it is usually sufficient to check for a 
	pub fn linearizable_read(&self) -> impl Future<Item=(), Error=Error> + Send {
		ok(())
	}


}


impl<R: Send + 'static> ServerShared<R> {

	pub fn run_tick<F, O>(shared: &Arc<Self>, f: F) -> O
		where F: FnOnce(&mut ServerState<R>, &mut Tick) -> O
	{
		let mut state = shared.state.lock().unwrap();

		// NOTE: Tick must be created after the state is locked to gurantee monotonic time always
		// XXX: We can reuse the same tick object many times if we really want to 
		let mut tick = Tick::empty();

		let out = f(&mut state, &mut tick);
		
		// In the case of a failure here, we want to attempt to backoff or demote ourselves from leadership
		// NOTE: We can survive short term disk failures as long as we know that there is metadata that has not been sent
		// Also splitting up 
		if let Err(e) = Self::finish_tick(shared, &mut state, tick) {
			// This should poison the state guard that we still hold and thus prevent any more progress from occuring
			// TODO: Eventually we can decompose exactly what failed and defer work to future retries
			panic!("Tick failed to finish: {:?}", e);
		}

		out
	}


	// TODO: If this fails, we may need to stop the server (silently ignoring failures may ignore the fact that metadata from previous rounds was not )
	// NOTE: This function assumes that the given state guard is for the exact same state as represented within this shared state
	fn finish_tick(shared: &Arc<Self>, state: &mut ServerState<R>, tick: Tick) -> Result<()> {

		let mut should_update_commit = false;


		// If new entries were appended, we must notify the flusher
		if tick.new_entries {

			// When our log has fewer entries than are committed, the commit index may go up
			// TODO: Will end up being a redundant operation with the below one
			should_update_commit = true;

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

			should_update_commit = true;
		}

		if should_update_commit {
			Self::update_commit_index(shared, &state);
		}

		// TODO: In most cases it is not necessary to persist the config unless we are doing a compaction, but we should schedule a task to ensurethat this gets saved eventually
		if tick.config {
			state.config_file.store(&rpc::marshal(ServerConfigurationSnapshotRef {
				config: state.inst.config_snapshot(),
				//routes: &state.routes
			})?)?;
		}

		// TODO: We currently assume that the ConsensusModule will always output a next_tick if it may have changed since last time. This is something that we probably need to verify in more dense
		if let Some(next_tick) = tick.next_tick {

			// Notify the cycler only if the next required tick is earlier than the last scheduled cycle 
			let next_cycle = state.scheduled_cycle.and_then(|time| {
				let next = tick.time + next_tick;
				if time > next {
					Some(next)
				}
				else {
					None
				}
			});

			if let Some(next) = next_cycle {
				// XXX: this is our only mutable reference to the state right now
				state.scheduled_cycle = Some(next);
				state.state_changed.notify();
			}
		}

		// TODO: A leader can dispatch RequestVotes in parallel to flushing its metadata so long as the metadata is flushed before it elects itself as the leader (aka befoer it processes replies to RequestVote)

		Self::dispatch_messages(&shared, &state, tick.messages);


		Ok(())
	}

	fn update_match_index(shared: &Arc<Self>) {
		// Getting latest match_index
		let cur_mi = shared.log.match_index().unwrap_or(0);
		let cur_mt = shared.log.term(cur_mi).unwrap();
		let cur = LogPosition {
			index: cur_mi,
			term: cur_mt
		};

		// Updating it
		let mut mi = shared.match_index.lock();
		// NOTE: The match_index is not necessarily monotonic in the case of log truncations
		if *mi != cur {
			*mi = cur;

			mi.notify_all();

			
			// TODO: It is annoying that this is in this function
			// On the leader, a change in the match index may cause the number of matches needed to be able to able the commit index
			// In the case of a single-node system, this let commits occur nearly immediately as no external requests need to be waited on in that case

			Self::run_tick(shared, |state, tick| state.inst.cycle(tick));
		}
	}


	/// Notifies anyone waiting on something to get committed
	/// TODO: Realistically as long as we enforce that it atomically goes up, we don't need to have a lock on the state in order to perform this update
	fn update_commit_index(shared: &Self, state: &ServerState<R>) {

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

	fn dispatch_messages(shared: &Arc<Self>, state: &ServerState<R>, messages: Vec<Message>) {
	
		if messages.len() == 0 {
			return;
		}


		let mut append_entries = vec![];
		let mut request_votes = vec![];

		// TODO: We should chain on some promise holding one side of a channel so that we can cancel this entire request later if we end up needing to 
		let new_request_vote = |
			to_id: ServerId, req: &RequestVoteRequest
		| {
			let shared = shared.clone();

			shared.client.call_request_vote(to_id, req)
			.timeout(Duration::from_millis(REQUEST_TIMEOUT))
			.then(move |res| -> FutureResult<(), ()> {
				
				Self::run_tick(&shared, |state, tick| {
					if let Ok(resp) = res {
						state.inst.request_vote_callback(to_id, resp, tick);
					}
				});				

				ok(())
			})

		};

		let new_append_entries = |
			to_id: ServerId, req: &AppendEntriesRequest, last_log_index: u64
		| {

			let shared = shared.clone();

			let ret = shared.client.call_append_entries(to_id, req)
			.timeout(Duration::from_millis(REQUEST_TIMEOUT))
			.then(move |res| -> FutureResult<(), ()> {

				Self::run_tick(&shared, |state, tick| {
					if let Ok(resp) = res {
						// NOTE: Here we assume that this request send everything up to and including last_log_index
						// ^ Alternatively, we could have just looked at the request object that we have in order to determine this
						state.inst.append_entries_callback(to_id, last_log_index, resp, tick);
					}
					else {
						state.inst.append_entries_noresponse(to_id, tick);
					}
				});
			
				ok(())
			});
			// TODO: In the case of a timeout or other error, we would still like to unblock this server from having a pending_request

			ret
		};

		for msg in messages {
			for to_id in msg.to {
				match msg.body {
					MessageBody::AppendEntries(ref req, ref last_log_index) => {
						append_entries.push(new_append_entries(to_id, req, *last_log_index));
					},
					MessageBody::RequestVote(ref req) => {
						request_votes.push(new_request_vote(to_id, req));
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
	pub fn wait_for_match<T: 'static>(shared: Arc<Self>, c: MatchConstraint<T>)
		-> impl Future<Item=T, Error=Error> + Send where T: Send
	{
		loop_fn((shared, c), |(shared, c)| {

			let (c, fut) = {
				let mi = shared.match_index.lock();

				// TODO: I don't think yields sufficient atomic gurantees

				let (c, pos) = match c.poll(shared.log.as_ref()) {
					ConstraintPoll::Satisfied(v) => return Either::A(ok(Loop::Break(v))),
					ConstraintPoll::Unsatisfiable => return Either::A(err("Halted progress on getting match".into())),
					ConstraintPoll::Pending(v) => v
				};

				(c, mi.wait(pos))
			};
			
			Either::B(fut.then(move |_| {
				ok(Loop::Continue((shared, c)))
			}))
		})
	}

	// Where will this still be useful: For environments where we just want to do a no-op or a change to the config but we don't really care about results

	/// TODO: We must also be careful about when the commit index
	/// Waits for some conclusion on a log entry pending committment
	/// This can either be from it getting comitted or from it becomming never comitted
	/// A resolution occurs once a higher log index is comitted or a higher term is comitted
	pub fn wait_for_commit(shared: Arc<Self>, pos: LogPosition)
		-> impl Future<Item=(), Error=Error> + Send
	{

		loop_fn((shared, pos), |(shared, pos)| {

			let waiter = {
				let c = shared.commit_index.lock();

				if c.term > pos.term || c.index >= pos.index {
					return Either::A(ok(Loop::Break(())));
				}

				c.wait(LogPosition { term: 0, index: 0 })
			};

			Either::B(waiter.then(move |_| {
				ok(Loop::Continue((shared, pos)))
			}))
		})

		// TODO: Will we ever get a request to truncate the log without an actual committment? (either way it isn't binding to the future of this proposal until it actually comitted something that is in conflict with us)
	}

	// TODO: wait_for_applied will basically end up mostly being absorbed into the callback system with the exception of 

	// NOTE: This is still somewhat relevant for blocking on a read index to be available

	/// Given a known to be comitted index, this waits until it is available in the state machine
	/// NOTE: You should always first wait for an item to be comitted before waiting for it to get applied (otherwise if the leader gets demoted, then the wrong position may get applied)
	pub fn wait_for_applied(shared: Arc<Self>, pos: LogPosition) -> impl Future<Item=(), Error=Error> + Send {

		loop_fn((shared, pos), |(shared, pos)| {

			let waiter = {
				let app = shared.last_applied.lock();
				if *app >= pos.index {
					return Either::A(ok( Loop::Break(()) ));
				}

				app.wait(pos.index)
			};

			Either::B(waiter.then(move |_| {
				ok(Loop::Continue((shared, pos)))
			}))
		})

	}


}

impl<R: Send + 'static> rpc::ServerService for Server<R> {

	fn client(&self) -> &rpc::Client {
		&self.shared.client
	}

	fn pre_vote(&self, req: RequestVoteRequest) -> rpc::ServiceFuture<RequestVoteResponse> { to_future_box!({
		let state = self.shared.state.lock().unwrap();
		let res = state.inst.pre_vote(req);
		Ok(res)
	}) }

	fn request_vote(&self, req: RequestVoteRequest) -> rpc::ServiceFuture<RequestVoteResponse> { to_future_box!({

		let res = ServerShared::run_tick(&self.shared, |state, tick| {
			state.inst.request_vote(req, tick)
		});

		Ok(res.persisted())
	})}
	
	fn append_entries(
		&self, req: AppendEntriesRequest
	) -> rpc::ServiceFuture<AppendEntriesResponse> {
		
		// TODO: In the case that entries are immediately written, this is overly expensive

		Box::new(to_future!({

			let res = ServerShared::run_tick(&self.shared, |state, tick| {
				state.inst.append_entries(req, tick)
			});

			Ok((self.shared.clone(), res?))

		}).and_then(|(shared, res)| {
			ServerShared::wait_for_match(shared, res)			
		}))
	}
	
	fn timeout_now(&self, req: TimeoutNow) -> rpc::ServiceFuture<()> { to_future_box!({

		ServerShared::run_tick(&self.shared, |state, tick| {
			state.inst.timeout_now(req, tick)
		})?;

		Ok(())

	}) }

	// TODO: This may become a ClientService method only? (although it is still sufficiently internal that we don't want just any old client to be using this)
	fn propose(&self, req: ProposeRequest) -> rpc::ServiceFuture<ProposeResponse> {

		Box::new(to_future!({

			let (data, wait) = (req.data, req.wait);

			let res = ServerShared::run_tick(&self.shared, |state, tick| {
				state.inst.propose_entry(data, tick)
			});

			// Ideally cascade down to a result and an error type
			if let Ok(prop) = res {
				Ok((wait, self.shared.clone(), prop))
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
