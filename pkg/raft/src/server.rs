
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

use std::collections::{HashMap, HashSet};
use std::time::{SystemTime, Duration};
use std::sync::{Arc, Mutex, MutexGuard};

use tokio::prelude::FutureExt;
use super::state_machine::StateMachine;

/// After this amount of time, we will assume that 
/// 
/// NOTE: This value doesn't matter very much, but the important part is that every single request must have some timeout associated with it to prevent the number of pending incomplete requests from growing indefinately in the case of other servers leaving connections open for an infinite amount of time (so that we never run out of file descriptors)
const REQUEST_TIMEOUT: u64 = 500;

// Basically whenever we connect to another node with a fresh connection, we must be able to negogiate with each the correct pair of cluster id and server ids on both ends otherwise we are connecting to the wrong server/cluster and that would be problematic (especially when it comes to aoiding duplicate votes because of duplicate connections)


/*
	Things that we have not generalized yet:
	- Storage beyond that of just config files that are BlobFiles
		- Not really a big deal right now

	- WriteEntries to log is an important operation
	- Ideally 

*/

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


// Basically anything that implements clone and can be moved around can be used

pub struct Server {
	shared: Arc<ServerShared>,
}

/// Server variables that can be shared by many different threads
struct ServerShared {

	state: Mutex<ServerState>,

	/// Holds the index of the log index most recently persisted to disk
	/// TODO: Should have the lock in
	match_index: Condition<u64, u64>, // < TODO: Also possible to have a lock-free version using an atomic variable 

	/// Holds the value of the current commit_index for the server
	/// A listener will be notified if we got a commit_index at least as up to date as their given position
	/// NOTE: The state machine will listen for (0,0) always so that it is always sent new entries to apply
	commit_index: Condition<u64, Proposal>,

}

/// All the mutable state for the server that you hold a lock in order to look at
struct ServerState {
	inst: ConsensusModule,

	meta_file: BlobFile, config_file: BlobFile,

	log: Arc<LogStorage + Send + Sync + 'static>,

	// XXX: Contain client connections

	// XXX: No point in these being locked

	// XXX: Another event will be used to send to the thread that flushes the log

	// It will emit back through the match_index log
	
	// commit_index will be peeked by the 

	/// Trigered whenever the state or configuration is changed
	/// Should be received by the cycler to update timeouts for heartbeats/elections
	state_event: EventSender, state_receiver: Option<EventReceiver>,

	/// Triggered whenever a new entry has been queued to be added to the log
	/// Should be received by the thread waiting that is performing flushes
	log_event: EventSender, log_receiver: Option<EventReceiver>,

	// For each observed server id, this is the last known address at which it can be found
	// This is used 
	// XXX: Cleaning up old routes: Everything in the cluster can always be durable for a long time
	// Otherwise, we will maintain up to 16 unused addresses to be pushed out on an LRU basis
	// because servers are only ever added one and a time and configurations should get synced quickly this should always be reasonable
	// The only complication would be new servers which won't have the entire config yet
	// We could either never delete servers with ids smaller than our lates log index and/or make sure that servers are always started with a complete configuration snaphot (which includes a snaphot of the config ips + routing information)
	routes: HashMap<ServerId, String>
}

impl Server {

	pub fn new(inst: ConsensusModule, log: Arc<LogStorage + Send + Sync + 'static>, meta_file: BlobFile, config_file: BlobFile) -> Self {
		// TODO: Don't forget to handle directory locking somewhere

		let (tx_state, rx_state) = event();
		let (tx_log, rx_log) = event();

		let mut routes = HashMap::new();

		routes.insert(1, "http://127.0.0.1:4001".to_string());
		routes.insert(2, "http://127.0.0.1:4002".to_string());

		let state = ServerState {
			inst,
			log,
			meta_file, config_file,
			state_event: tx_state, state_receiver: Some(rx_state),
			log_event: tx_log, log_receiver: Some(rx_log),
			routes
		};

		let shared = ServerShared {
			state: Mutex::new(state),

			// TODO: Have better initial values
			match_index: Condition::new(0),
			commit_index: Condition::new(0)
		};

		Server {
			shared: Arc::new(shared)
		}
	}

	// THis will need to be the thing running a tick method which must just block for events

	// NOTE: If we also give it a state machine, we can do that for people too
	pub fn start(server_handle: Arc<Server>) -> impl Future<Item=(), Error=()> + Send + 'static {

		let (id, state_event, log_event) = {
			let mut state = server_handle.shared.state.lock().expect("Failed to lock instance");

			(
				state.inst.id(),

				// If these errors out, then it means that we tried to start the server more than once
				state.state_receiver.take().expect("State receiver already taken"),
				state.log_receiver.take().expect("Log receiver already taken")
			)
		};

		let service = rpc::run_server::<Arc<Server>, Server>(4000 + (id as u16), server_handle.clone());

		let cycler = Self::run_cycler(&server_handle, state_event);
		let matcher = Self::run_matcher(&server_handle, log_event);

		// TODO: Finally if possible we should attempt to broadcast our ip address to other servers so they can rediscover us

		service
		// NOTE: Because in bootstrap mode a server can spawn requests immediately without the first futures cycle, it may spawn stuff before tokio is ready, so we must make this lazy
		.join3(
			lazy(|| cycler),
			lazy(|| matcher)
		)
		.map(|_| ()).map_err(|_| ())
	}

	/// Runs the idle loop for managing the server and maintaining leadership, etc. in the case that no other events occur to drive the server
	fn run_cycler(
		server_handle: &Arc<Server>,
		state_event: EventReceiver
	) -> impl Future<Item=(), Error=()> + Send + 'static {

		let shared_handle = server_handle.shared.clone();
		
		loop_fn((shared_handle, state_event), |(shared_handle, state_event)| {

			// TODO: Switch to an Instant and use this one time for this entire loop for everything
			//let now = Instant::now();

			let mut wait_time = Duration::from_millis(0);
			{

				let mut state = shared_handle.state.lock().expect("Failed to lock server instance");

				let mut tick = Tick::empty();
				state.inst.cycle(&mut tick);

				// XXX: Now perform the effect of the output thing


				// TODO: Should be switched to a tokio::timer which doesn't block anything
				// NOTE: We take it so that the finish_tick doesn't re-trigger this loop and prevent sleeping all together
				if let Some(d) = tick.next_tick.take() {
					wait_time = d;
				}
				else {
					// TODO: Ideally refactor to represent always having a next time as part of every operation
					eprintln!("Server cycled with no next tick time");
				}

				// Now tick must have a reference to the 

				ServerShared::finish_tick(&shared_handle, state, tick);

			}

			//if false {
			//	return ok(Loop::Break(()));
			//}

			println!("Sleep {:?}", wait_time);

			// TODO: If not necessary, we should be able to support zero-wait cycles (although now those should never happen as the consensus module should internally now always converge in one run)
			state_event.wait(wait_time).map(move |state_event| {
				Loop::Continue((shared_handle, state_event))
			})
		})
		.map_err(|_| {
			// XXX: I think there is a stray timeout error that could occur here
			()
		})
	}

	/// Flushes log entries to persistent storage as they come in
	fn run_matcher(
		server_handle: &Arc<Server>,
		log_event: EventReceiver
	) -> impl Future<Item=(), Error=()> + Send + 'static {

		let shared_handle = server_handle.shared.clone();

		loop_fn((shared_handle, log_event), |(shared_handle, log_event)| {

			let cur_mi = {
				let state = shared_handle.state.lock().unwrap();
				state.log.flush();

				state.log.match_index().unwrap_or(0)
			};

			{
				let mut mi = shared_handle.match_index.lock();

				if *mi != cur_mi {
					*mi = cur_mi;

					mi.notify_all();
				}
			}
			
			// TODO: These is generally no good reason to wake up for any timeout 
			log_event.wait(Duration::from_secs(10)).map(move |log_event| {
				Loop::Continue((shared_handle, log_event))
			})
		})

	}

	/// When entries are comitted, this will apply them to the state machine
	fn run_applier(
		server_handle: &Arc<Server>
	) {

		// TODO

	}


}


impl ServerShared {


	// TODO: If this fails, we may need to stop the server
	// NOTE: This function assumes that the given state guard is for the exact same state as represented within this shared state
	pub fn finish_tick<'a>(shared: &Arc<Self>, state: MutexGuard<'a, ServerState>, tick: Tick) -> Result<()> {

		// If new entries were appended, we must notify the flusher
		// XXX: Single sender for just the 

		if tick.meta {
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
		if tick.next_tick.is_some() {
			state.state_event.notify();
		}

		Ok(())
	}

	/// Notifies anyone waiting on something to get committed
	fn update_commit_index(shared: &ServerShared, state: &ServerState) {

		let latest_ci = state.inst.meta().commit_index;
		let mut ci = shared.commit_index.lock();

		// NOTE '<' should be sufficent here as the commit index should never go backwards
		if *ci != latest_ci {
			*ci = latest_ci;

			// TODO: Run a filtered notify based on the committed term and index
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

}

macro_rules! to_future {
    ($x:block) => ({
        Box::new(match (move || $x)() {
			Ok(v) => ok(v),
			Err(e) => err(e)
        })
	})
}


impl rpc::ServerService for Server {

	fn pre_vote(&self, req: RequestVoteRequest) -> rpc::ServiceFuture<RequestVoteResponse> { to_future!({
		let mut state = self.shared.state.lock().unwrap();
		
		// NOTE: Tick must be created after the state is locked to gurantee monotonic time always
		let mut tick = Tick::empty();
		let res = state.inst.pre_vote(req, &mut tick);

		// Hopefully no messages were produced, we may only have anew hard state, but this is no immediate need to definitely flush it

		ServerShared::finish_tick(&self.shared, state, tick)?;

		Ok(res)
	}) }

	fn request_vote(&self, req: RequestVoteRequest) -> rpc::ServiceFuture<RequestVoteResponse> { to_future!({
		let mut state = self.shared.state.lock().unwrap();

		let mut tick = Tick::empty();
		let res = state.inst.request_vote(req, &mut tick);

		ServerShared::finish_tick(&self.shared, state, tick)?;

		Ok(res.persisted())
	})}
	
	fn append_entries(
		&self, req: AppendEntriesRequest
	) -> rpc::ServiceFuture<AppendEntriesResponse> { to_future!({

		let mut state = self.shared.state.lock().unwrap();

		let mut tick = Tick::empty();
		let res = state.inst.append_entries(req, &mut tick)?;

		ServerShared::finish_tick(&self.shared, state, tick)?;

		// TODO: Given a server instance, we can convert any constraint into a future
		// Basically a persistent 
		let res = match res.poll() {
			ConstraintPoll::Satisfied(v) => v,

			// TODO: Must wait on the match_index event if allowed
			_ => return Err("Not implemented".into())
		};
		

		Ok(res)
	})}
	
	fn timeout_now(&self, req: TimeoutNow) -> rpc::ServiceFuture<()> { to_future!({

		let mut state = self.shared.state.lock().unwrap();

		let mut tick = Tick::empty();
		state.inst.timeout_now(req, &mut tick)?;

		ServerShared::finish_tick(&self.shared, state, tick)?;

		Ok(())

	}) }

	// TODO: This may become a ClientService method only? (although it is still sufficiently internal that we don't want just any old client to be using this)
	fn propose(&self, req: ProposeRequest) -> rpc::ServiceFuture<ProposeResponse> { to_future!({
		let mut state = self.shared.state.lock().unwrap();

		let mut tick = Tick::empty();
		let res = state.inst.propose_entry(req.data, &mut tick);

		ServerShared::finish_tick(&self.shared, state, tick)?;

		/*
		// TODO: This may be somewhat inefficient to relock if this is a single node cluster and is able to commit immediately 
		let ci = self.shared.commit_index.lock();
		ci.wait(prop).and_then(|_| {
			// Reacquire the lock and see if we were able to make progress or if we are just done for
		})
		*/

		// XXX: This will be very similar to the match constraint in terms of checking indexes

		//self.shared.commit_index.

		// Here we would ideally want to be able to block until it is comitted (only possible from a current member)
		// ^ Although this is really not our job
		// ^ THis should 

		if let ProposeResult::Started(prop) = res {
			Ok(ProposeResponse {
				term: prop.term,
				index: prop.index
			})
		}
		else {
			println!("propose result: {:?}", res);
			Err("Not implemented".into())
		}
	}) }

}
