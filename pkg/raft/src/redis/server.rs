
extern crate tokio;

use super::resp::*;
use tokio::prelude::*;
use tokio::io::copy;
use tokio::net::{TcpListener, TcpStream};
use tokio::codec::Framed;

use futures::prelude::*;
use futures::future::*;
use futures::{Stream, Future};
use futures::stream::SplitSink;
use raft::errors::*;
use std::sync::{Mutex, Arc};
use futures::sync::mpsc;
use std::collections::{HashMap, HashSet};

// TODO: Current issue: We currently don't support disabling pub/sub functionality all together as we currently don't decouple the low-level stream nature of those commands, so if you want to implement a server without pub/sub, then you need to implement handlers 
// ^ Additionally our service overlay is totally stateless and does not deal with client semantics right now (which is problematic for transactions )


/// The response to a typical request/response model command
pub type CommandResponse = Box<Future<Item=RESPObject, Error=Error> + Send>;

/// The response to a push (aka pub/sub) model command
/// Only ever really used internally for certain commands
type CommandStream = Box<Stream<Item=PushObject, Error=Error> + Send>;


pub trait Service {

	fn get(&self, key: RESPString) -> CommandResponse;

	fn set(&self, key: RESPString, value: RESPString) -> CommandResponse;

	fn del(&self, key: RESPString) -> CommandResponse;

	/// Given a message, this should send it to all remote clients for that message
	fn publish(&self, channel: RESPString, object: RESPObject) -> Box<Future<Item=usize, Error=Error> + Send>;

	/// Called whenever a client subscribes to a channel locally
	fn subscribe(&self, channel: RESPString) -> Box<Future<Item=(), Error=Error> + Send>;

	/// Called whenever a client unsubscribes from a channel locally
	fn unsubscribe(&self, channel: RESPString) -> Box<Future<Item=(), Error=Error> + Send>;

}


enum PushObject {
	Message(RESPString, RESPObject),
	Subscribe(RESPString, usize),
	Unsubscribe(RESPString, usize),
	Pong(RESPString)
}

enum Packet {
	Push(PushObject),
	Response(RESPObject)
}

type ChannelName = Vec<u8>;
type ClientId = u64;

enum ServerClientMode {
	RequestResponse,
	Push
}


struct ServerClient {
	id: ClientId,
	//mode: ServerClientMode,

	/// A handle for pushing packets to this client
	/// NOTE: Only push messages should be sendable through this interface
	sender: mpsc::Sender<(RESPString, RESPObject)>,
	
	/// All channels which this client is subscribed to
	/// TODO: Probably cheaper to assign each channel a unique id to avoid having to store many copies to its name (or represent it as a shared Bytes array)
	channels: HashSet<ChannelName>
}

//pub type CommandHandler<T> = (Fn(&T, RESPCommand) -> CommandResponse) + Sync;


/// Internal return value for representing the raw output of trying to execute a command
enum CommandResult {
	Resp(CommandResponse),
	
	Push(CommandStream),

	/// An immediately available resposne
	Imm(RESPObject),

	Fail(Error)
}


pub struct Server<T: 'static> {
	service: T,
	state: Mutex<ServerState>
}

struct ServerState {
	last_id: ClientId,

	/// All clients connected to this server
	clients: HashMap<ClientId, Arc<Mutex<ServerClient>>>,

	/// Listing of all clients in each channel
	channels: HashMap<ChannelName, HashSet<ClientId>>,
}


/*
	The main issue:
	- A client that is subscribing may end up blocking while subscriptions are running (because we must query the service)
		- The better idea:
		- Must be able to split off the stream and respond 

	- End up selecting between three futures:
		- Future 1: Either<Receive Request, Respond To Request>
		- Future 2: Either<mpsc poll, Never>
			- Ideally only poll for the mpsc upon first transition to becoming a subscriber
			- This will also allow us to gurantee that we don't read from the mpsc until we are actually latched into this mode
*/

impl<T: 'static> Server<T>
	where T: Service + Send + Sync
{

	pub fn new(service: T) -> Self {
		Server {
			service,
			state: Mutex::new(ServerState {
				last_id: 0,
				clients: HashMap::new(),
				channels: HashMap::new()
			})
		}
	}

	pub fn start(inst: Arc<Self>, port: u16) -> impl Future<Item=(), Error=()> {
		let addr = format!("127.0.0.1:{}", port).parse().unwrap();
		let listener = TcpListener::bind(&addr)
			.expect("unable to bind TCP listener");

		let server = listener.incoming()
			.map_err(|e| eprintln!("accept failed = {:?}", e))
			.for_each(move |sock| {
				tokio::spawn(Self::handle_connection(inst.clone(), sock))
			});

		server
	}

	/// Publishes a messages to all clients connected to the local server
	/// Resolves with the number of clients that were notified
	/// TODO: from_id is trivially not necessary as a publisher should never be in a subscriber mode
	pub fn publish(
		&self, channel: ChannelName, obj: RESPObject
	) -> impl Future<Item=usize, Error=()> {
		let state = self.state.lock().unwrap();

		let client_ids = match state.channels.get(&channel) {
			Some(arr) => arr,
			None => return Either::A(ok(0))
		};

		let futs = client_ids.iter().filter_map(|id| {

			let mut client = match state.clients.get(id) {
				Some(c) => c.lock().unwrap(),
				None => return None // Inconsistent map
			};

			// TODO: Possibly convert into an unbounded sender if we are going to clone it anyway
			Some(client.sender.clone().send(
				(channel.clone().into(), obj.clone())
			))
		}).collect::<Vec<_>>();

		let num = futs.len();

		Either::B(join_all(futs)
		.map(move |_| num)
		.map_err(|_| ()))
	}

	fn handle_connection(inst: Arc<Self>, sock: TcpStream) -> impl Future<Item=(), Error=()> + Send {

		sock.set_nodelay(true).expect("Failed to set nodelay");
		//sock.set_recv_buffer_size(128).expect("Failed to set rcv buffer");

		let framed_sock = Framed::new(sock, RESPCodec::new());
		let (sink, stream) = framed_sock.split();

		let (tx, rx) = mpsc::channel::<(RESPString, RESPObject)>(16);

		let client = {
			let mut server_state = inst.state.lock().unwrap();

			server_state.last_id += 1;
			let id = server_state.last_id;

			let client = Arc::new(Mutex::new(ServerClient {
				id,
				channels: HashSet::new(),
				sender: tx
			}));

			server_state.clients.insert(id, client.clone());

			println!("Start conn {}", id);

			client
		};


		enum Event {
			Request(RESPObject),
			Message(RESPString, RESPObject),
			End
		}
	

		let f = stream
			.map(|req| Event::Request(req))
			.map_err(|e| Error::from(e))
			.chain(futures::stream::once(Ok(Event::End)))
		.select(
			rx
			.map(|(channel, pkt)| Event::Message(channel, pkt))
			.map_err(|_| panic!("Unexpected mpsc error"))	
		)
		.take_while(|item| {
			Ok(match item {
				Event::End => false,
				_ => true
			})
		})
		// Next step is to ensure that this never fails so that we can cleanup with one instance of the client
		.fold((inst.clone(), client.clone(), sink, false), |(inst, client, sink, is_push), item| {

			// Get the next packet(s) to send
			let out = match item {
				Event::Request(req) => {
					
					let cmd = match req.into_command() {
						Ok(c) => c,
						Err(e) => return Either::A(err(e.into()))
					};

					let res = match Self::run_command(&inst, &client, is_push, cmd) {
						Ok(v) => v, Err(v) => v
					};

					let out: Box<Stream<Item=Packet, Error=Error> + Send> = match res {
						CommandResult::Imm(v) => Box::new(ok(v).map(|r| Packet::Response(r)).into_stream()),
						CommandResult::Resp(v) => Box::new(v.map(|r| Packet::Response(r)).into_stream()),
						CommandResult::Push(s) => Box::new(s.map(|r| Packet::Push(r))),
						CommandResult::Fail(e) => return Either::A(err(e))
					};

					out
				},
				Event::Message(channel, message) => {
					Box::new(ok(Packet::Push(PushObject::Message(channel, message))).into_stream())
				},

				// This should have been absorbed by our above clause
				Event::End => panic!("Should not have gotten this far")
			};

			// Send them
			// TODO: Currently this means that a blocking request will prevent more messages to be taken out of the mpsc (the solution to this would be to select on a list of promises which would change after each cycle if the response produces a new promise)
			Either::B(Self::handle_connection_sender(out, sink, is_push).and_then(|(sink, is_push)| {
				ok((inst, client, sink, is_push))
			}))
		});

		f
		.map_err(|e| {

			// Ignoring typical errors
			if let Error(ErrorKind::Io(ref eio), _) = e {
				// This is triggered by a client that disconnects early while we are sending it data
				if eio.kind() == std::io::ErrorKind::ConnectionReset {
					return ();
				}
			}

			eprintln!("Client Error: {:?}", e)
		})
		.map(|v| ())
		.then(move |_| {
			Self::cleanup_client(inst, client)
			.map_err(|e| {
				eprintln!("Error while disconnecting {:?}", e)
			})
		})

	}

	/// Responsible for all sending of responses/pushes back to the client
	/// Waits for packets on a shared mpsc to come from the response server and from external clients and serially sends them back through the tcp connection
	fn handle_connection_sender(
		out: Box<Stream<Item=Packet, Error=Error> + Send>, sink: SplitSink<Framed<TcpStream, RESPCodec>>, is_push: bool
	) -> impl Future<Item=(SplitSink<Framed<TcpStream, RESPCodec>>, bool), Error=Error> + Send
	{

		out.fold((sink, is_push), |(sink, mut is_push), pkt| {

			let obj = match pkt {
				Packet::Push(push) => {

					match push {
						PushObject::Message(channel, msg) => {
							if !is_push {
								return Either::A(ok((sink, is_push)));
							}

							RESPObject::Array(vec![
								RESPObject::BulkString(b"message"[..].into()),
								RESPObject::BulkString(channel.into()),
								msg
							])
						},
						PushObject::Subscribe(channel, count) => {
							// The first subscribe should make us
							if count > 0 {
								is_push = true;
							}

							RESPObject::Array(vec![
								RESPObject::BulkString(b"subscribe"[..].into()),
								RESPObject::BulkString(channel.into()),
								RESPObject::Integer(count as i64)
							])
						},
						PushObject::Unsubscribe(channel, count) => {
							if count == 0 {
								is_push = false;
							}

							RESPObject::Array(vec![
								RESPObject::BulkString(b"unsubscribe"[..].into()),
								RESPObject::BulkString(channel.into()),
								RESPObject::Integer(count as i64)
							])
						},
						PushObject::Pong(data) => {
							RESPObject::Array(vec![
								RESPObject::BulkString(b"pong"[..].into()),
								RESPObject::BulkString(data.into())
							])
						}
					}
				},
				Packet::Response(obj) => {
					if is_push {
						// Generally this means that the client is not writes things in the right order
						return Either::A(err("Rejected to send request response in push mode".into()));
					}

					obj
				}
			};

			Either::B(sink.send(obj).map_err(|e| Error::from(e)).and_then(move |sink| {
				ok((sink, is_push))
			}))

		})
	}

	fn cleanup_client(
		inst: Arc<Self>, client: Arc<Mutex<ServerClient>>
	) -> impl Future<Item=(), Error=Error> {

		let (id, channels) = {
			let client = client.lock().unwrap();
			(client.id, client.channels.iter().map(|s| RESPString::from(s.clone())).collect::<Vec<_>>())
		};

		Self::run_command_unsubscribe(&inst, &client, &channels)
		.collect()
		.then(move |res| {
			// TODO: Make sure that this always happens regardless of errors

			// Now that all channels are unsubscribed, we can remove the client compltely
			let mut state = inst.state.lock().unwrap();
			state.clients.remove(&id);

			println!("Client disconnected!");

			res
		})
		.map(|_| ())
	}

	/// TODO: Must also implement errors for running commands that don't work in the current mode (currently the responses will cause failures anyway though)
	fn run_command(
		inst: &Arc<Self>, client: &Arc<Mutex<ServerClient>>, is_push: bool, cmd: RESPCommand
	) -> std::result::Result<CommandResult, CommandResult> {

		use self::CommandResult::*;

		if cmd.len() == 0 {
			return Ok(Imm(RESPObject::Error(b"No command specified"[..].into())));
		}

		let name = match std::str::from_utf8(cmd[0].as_ref()) {
			Ok(v) => v,
			// TODO: Should this immediately close the connection with a real error
			_ => return Ok(Imm(RESPObject::Error(b"Invalid command format"[..].into())))
		};

		// Normalize the name of the command
		let name_norm = name.to_uppercase();

		const MAX_ARG: usize = 100;
		let arity = |min: usize, max: usize| -> std::result::Result<(), CommandResult> {
			let valid = cmd.len() >= min && cmd.len() <= max;

			if valid {
				Ok(())
			}
			else {
				Err(Imm(RESPObject::Error(
					format!("ERR wrong number of arguments for '{}' command", name).as_bytes().into()
				)))
			}
		};

		let out = match name_norm.as_str() {
			"GET" => {
				arity(2, 2)?;
				Resp(inst.service.get(cmd[1].clone()))
			},
			"DEL" => {
				arity(2, 2)?;
				Resp(inst.service.del(cmd[1].clone()))
			},
			"SET" => {
				arity(3, 3)?;
				Resp(inst.service.set(cmd[1].clone(), cmd[2].clone()))
			},
			"SUBSCRIBE" => {
				arity(2, MAX_ARG)?;
				Push(Self::run_command_subscribe(inst, client, &cmd[1..]))
			},
			"UNSUBSCRIBE" => {
				arity(2, MAX_ARG)?;
				Push(Self::run_command_unsubscribe(inst, client, &cmd[1..]))
			},
			"PUBLISH" => {
				arity(3, 3)?;
				Resp(Self::run_command_publish(inst, client, cmd[1].clone(), cmd[2].clone()))
			},
			"COMMAND" => {
				arity(1, 1)?;
				Imm(RESPObject::SimpleString(b"OK"[..].into()))
			},
			"PING" => {
				arity(1, 2)?;

				if is_push {
					if cmd.len() == 1 {
						Push(Box::new(
							ok(PushObject::Pong(RESPString::from(vec![]))).into_stream()
						))
					}
					else {
						Push(Box::new(
							ok(PushObject::Pong(cmd[1].clone())).into_stream()
						))
					}
				}
				else {
					if cmd.len() == 1 {
						Imm(RESPObject::SimpleString(b"PONG"[..].into()))
					}
					else {
						Imm(RESPObject::BulkString(cmd[1].clone().into()))
					}
				}
			},
			_ => Imm(RESPObject::Error(
				format!("ERR unknown command '{}'", name).as_bytes().into()
			))
		};

		Ok(out)
	}

	fn run_command_subscribe(
		inst: &Arc<Self>, client: &Arc<Mutex<ServerClient>>, channels: &[RESPString]
	) -> CommandStream {

		let inst = inst.clone();
		let client = client.clone();

		let res = {
			let mut state = inst.state.lock().unwrap();
			let mut client = client.lock().unwrap();

			channels.iter()
			.map(|c| {
				let cur_subscribed = client.channels.contains(c.as_ref());

				let changed = if !cur_subscribed {
					client.channels.insert(c.as_ref().to_vec());

					let global_channels = {
						if !state.channels.contains_key(c.as_ref()) {
							state.channels.insert(c.as_ref().to_vec(), HashSet::new());
						}

						state.channels.get_mut(c.as_ref()).unwrap()
					};

					assert!(global_channels.insert(client.id));

					true
				}
				else {
					false
				};


				(c.clone(), client.channels.len(), changed)
			}).collect::<Vec<_>>()
		};

		let s = stream::iter_ok(res)
		.and_then(move |(c, client_count, changed)| {

			let ret = PushObject::Subscribe(c.clone(), client_count);

			if changed {
				Either::A(inst.service.subscribe(c).and_then(|_| {
					ok(ret)
				}))
			}
			else {
				Either::B(ok(ret))
			}
		});

		Box::new(s)
	}

	fn run_command_unsubscribe(
		inst: &Arc<Self>, client: &Arc<Mutex<ServerClient>>, channels: &[RESPString]
	) -> CommandStream {

		let inst = inst.clone();
		let client = client.clone();

		let res = {
			let mut state = inst.state.lock().unwrap();
			let mut client = client.lock().unwrap();

			channels.into_iter()
			.map(|c| {
				let cur_subscribed = client.channels.contains(c.as_ref());

				let changed = if cur_subscribed {
					client.channels.remove(c.as_ref());

					let last_removed = {
						let global_channels = state.channels.get_mut(c.as_ref()).unwrap();
						global_channels.remove(&client.id);

						global_channels.len() == 0
					};
					
					if last_removed {
						state.channels.remove(c.as_ref());
					}

					true
				}
				else {
					false
				};

				(c.clone(), client.channels.len(), changed)
			}).collect::<Vec<_>>()
		};


		let s = stream::iter_ok(res)
		.and_then(move |(c, client_count, changed)| {

			let ret = PushObject::Unsubscribe(c.clone(), client_count);

			if changed {
				Either::A(inst.service.unsubscribe(c).and_then(|_| {
					ok(ret)
				}))
			}
			else {
				Either::B(ok(ret))
			}
		});

		Box::new(s)	
	}


	fn run_command_publish(
		inst: &Arc<Self>, client: &Arc<Mutex<ServerClient>>, channel: RESPString, message: RESPString
	) -> CommandResponse {

		let inst = inst.clone();

		let obj = RESPObject::BulkString(message.into());

		let f = inst
		.publish(channel.as_ref().to_vec(), obj.clone())
		.map_err(|_| Error::from("Failed to publish message"))
		.and_then(move |num_local| {

			inst.service
			.publish(channel, obj)
			.and_then(move |num_remote| {
				let num = num_local + num_remote;
				ok(RESPObject::Integer(num as i64))
			})
		});

		Box::new(f)
	}

}

