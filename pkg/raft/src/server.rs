
use super::errors::*;
use super::protos::*;
use super::consensus::*;
use super::rpc;
use super::sync::*;
use std::time::Instant;
use futures::future::*;
use futures::{Future, Stream};

use hyper::{Body, Response};
use std::collections::{HashMap, HashSet};
use std::time::{SystemTime, Duration};
use std::sync::{Arc, Mutex};

use tokio::prelude::FutureExt;

use std::fs::{File, OpenOptions};
use super::state_machine::StateMachine;

/// After this amount of time, we will assume that 
/// 
/// NOTE: This value doesn't matter very much, but the important part is that every single request must have some timeout associated with it to prevent the number of pending incomplete requests from growing indefinately in the case of other servers leaving connections open for an infinite amount of time (so that we never run out of file descriptors)
const REQUEST_TIMEOUT: u64 = 500;



pub struct Server {
	shared: Arc<Mutex<ServerShared>>
}

struct ServerShared {
	inst: ConsensusModule,

	/// Trigered whenever the state or configuration is changed
	/// Should be received by the cycler to update timeouts for heartbeats/elections
	state_event: EventSender,

	// For each observed server id, this is the last known address at which it can be found
	// This is used 
	routes: HashMap<ServerId, String>
}


// This will be what actually produces stuff doesn't it

impl Server {

	pub fn new(inst: ConsensusModule) -> (Self, EventReceiver) {
		// TODO: Don't forget to handle directory locking somewhere

		let (tx_state, rx_state) = event();

		let mut routes = HashMap::new();

		routes.insert(1, "http://127.0.0.1:4001".to_string());
		routes.insert(2, "http://127.0.0.1:4002".to_string());

		let server = Server {
			shared: Arc::new(Mutex::new(ServerShared {
				inst,
				state_event: tx_state,
				routes
			}))
		};

		(server, rx_state)
	}

	// THis will need to be the thing running a tick method which must just block for events

	// NOTE: If we also give it a state machine, we can do that for people too
	pub fn start(server_handle: Arc<Server>, event: EventReceiver) -> impl Future<Item=(), Error=()> + Send + 'static {

		let id = server_handle.shared.lock().expect("Failed to lock instance").inst.id();
		let service = rpc::run_server(4000 + (id as u16), server_handle.clone());

		// General loop for managing the server and maintaining leadership, etc.
		
		// NOTE: Because in bootstrap mode a server can spawn requests immediately without the first futures cycle, it may spawn stuff before tokio is ready, so we must make this lazy
		let cycler = lazy(|| loop_fn((server_handle, event), |(server_handle, event)| {

			// TODO: Switch to an Instant and use this one time for this entire loop for everything
			//let now = Instant::now();

			let mut wait_time = Duration::from_millis(0);
			{

				let mut server = server_handle.shared.lock().expect("Failed to lock server instance");

				let mut tick = Tick::empty();

				// TODO: Ideally the cycler should a time as input
				server.inst.cycle(&mut tick);

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


				server.finish_tick(server_handle.shared.clone(), tick);
			}

			//if false {
			//	return ok(Loop::Break(()));
			//}

			println!("Sleep {:?}", wait_time);

			// TODO: If not necessary, we should be able to support zero-wait cycles (although now those should never happen as the consensus module should internally now always converge in one run)
			event.wait(wait_time).map(move |event| {
				Loop::Continue((server_handle, event))
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

}

impl ServerShared {

	fn dispatch_messages(&self, server_handle: Arc<Mutex<ServerShared>>, messages: Vec<Message>) {
	
		// TODO: Why are we initially sending a message in an empty server

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
			let server_handle = server_handle.clone();

			rpc::call_request_vote(addr, req)
			.timeout(Duration::from_millis(REQUEST_TIMEOUT))
			.then(move |res| -> FutureResult<(), ()> {
				
				let mut server = server_handle.lock().expect("Failed to lock instance");
				let mut tick = Tick::empty();

				if let Ok(resp) = res {
					server.inst.request_vote_callback(to_id, resp, &mut tick);
				}

				server.finish_tick(server_handle.clone(), tick);

				ok(())
			})

		};

		let new_append_entries = |
			to_id: ServerId, addr: &String, req: &AppendEntriesRequest, last_log_index: u64
		| {

			let server_handle = server_handle.clone();

			let ret = rpc::call_append_entries(addr, req)
			.timeout(Duration::from_millis(REQUEST_TIMEOUT))
			.then(move |res| -> FutureResult<(), ()> {

				let mut server = server_handle.lock().unwrap();
				let mut tick = Tick::empty();

				if let Ok(resp) = res {
					// NOTE: Here we assume that this request send everything up to and including last_log_index
					// ^ Alternatively, we could have just looked at the request object that we have in order to determine this
					server.inst.append_entries_callback(to_id, last_log_index, resp, &mut tick);
				}
				else {
					server.inst.append_entries_noresponse(to_id, &mut tick);
				}

				server.finish_tick(server_handle.clone(), tick);
			
				ok(())
			});
			// TODO: In the case of a timeout or other error, we would still like to unblock this server from having a pending_request

			ret
		};

		for msg in messages {
			for to_id in msg.to {

				// XXX: Assumes well known routes
				// ^ if this results in a miss, then this is a good insective to ask some other server for a new list of server addrs immediately
				let addr = self.routes.get(&to_id).unwrap();

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

	fn finish_tick(&mut self, server_handle: Arc<Mutex<ServerShared>>, tick: Tick) {

		if tick.meta {
			// TODO: Flush meta to disk
		}

		self.dispatch_messages(server_handle, tick.messages);

		// TODO: Verify this encapsulates all cases of meaningful state changes
		if tick.next_tick.is_some() {
			self.state_event.notify();
		}
	}




}

impl rpc::ServerService for Server {

	fn pre_vote(&self, req: RequestVoteRequest) -> Result<RequestVoteResponse> {
		let mut server = self.shared.lock().unwrap();

		let mut tick = Tick::empty();
		let res = server.inst.pre_vote(req, &mut tick);

		// Hopefully no messages were produced, we may only have anew hard state, but this is no immediate need to definitely flush it
		server.finish_tick(self.shared.clone(), tick);

		Ok(res)
	}

	fn request_vote(&self, req: RequestVoteRequest) -> Result<RequestVoteResponse> {
		let mut server = self.shared.lock().unwrap();

		let mut tick = Tick::empty();
		let res = server.inst.request_vote(req, &mut tick);

		// Probably just the ticker is needed here
		server.finish_tick(self.shared.clone(), tick);

		Ok(res)
	}
	
	fn append_entries(&self, req: AppendEntriesRequest) -> Result<AppendEntriesResponse> {
		let mut server = self.shared.lock().unwrap();

		let mut tick = Tick::empty();
		
		let res = server.inst.append_entries(req, &mut tick)?;

		server.finish_tick(self.shared.clone(), tick);

		// Block until the state machine is fully applied (with a future) if we received an index

		Ok(res.unwrap())
	}
	
	fn timeout_now(&self, req: TimeoutNow) -> Result<()> {
		let mut server = self.shared.lock().unwrap();

		let mut tick = Tick::empty();
		
		server.inst.timeout_now(req, &mut tick)?;

		server.finish_tick(self.shared.clone(), tick);

		Ok(())
	}

	// TODO: This may become a ClientService method only? (although it is still sufficiently internal that we don't want just any old client to be using this)
	fn propose(&self, req: ProposeRequest) -> Result<ProposeResponse> {
		let mut server = self.shared.lock().unwrap();

		let mut tick = Tick::empty();
		
		let res = server.inst.propose_entry(req.data, &mut tick);

		server.finish_tick(self.shared.clone(), tick);

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
	}

}


// /// // / // / // XXXX: Old stuff below here // // / // // / /// / // / / / /


/*
	Files
	- `/log` <- append-only log (with the exception of compactions which we may either implement as new files )
		- we will generally always hold at most two log files and at most two snapshot files
	- `/config`
		- If the block size is small enough, and we assert that, then the config 
	- `/meta`
		- 
*/

/*
	Other scenarios
	- Server startup
		- Server always starts completely idle and in a mode that would reject external requests
		- If we have configuration on disk already, then we can use that
		- If we start with a join cli flag, then we can:
			- Ask the cluster to create a new unique machine id (we could trivially use an empty log entry and commit that to create a new id) <- Must make sure this does not conflict with the master's id if we make many servers before writing other data
	
		- If we are sent a one-time init packet via http post, then we will start a new cluster on ourselves

*/
