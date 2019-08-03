use super::errors::*;
use super::protos::*;
use super::routing::*;
use futures::future::*;
use futures::{Future, Stream};
use futures::prelude::*;
use futures::prelude::await;
use hyper::{Body};
use serde::{Deserialize, Serialize};
use bytes::Bytes;
use std::borrow::Borrow;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::SystemTime;
use std::str::FromStr;

/*
	Helpers for making an RPC server for communication between machines
	- Similar to gRPC, we currently support metadata key-value pairs that are added to a request
	- But, we also support metadata to be returned in the response as well separately from the return value

	NOTE: Not for handling external interfaces. Primarily for keepng 
*/

/*
	General considerations for the client/routing implementation:

	A Client can be in one of three modes:
	- Anonymous
	- ClusterClient
		- Contains at least a cluster_id (and possibly also routes)
		- Created from the Anonymous type upon observing a ClusterId
	- ClusterMember
		- Contains all of a ClusterClient along with a self identity
		- Created from a ClusterClient when a server is started up

	- What we want to avoid is the fourth trivial state that is an identity/or/routes without a cluster_id
		- Such should be synactically invalid
		- Anyone making a request can alter this setup

	The discovery service may start in any of these modes but will long term operate as a ClusterClient or ClusterMember

	A consensus Server will always be started with a configuration in ClusterMember mode


	External clients looking to just use us as a service will use initially a Anonymous identity but will bind to the first cluster id that they see and then will maintain that for as long as they retained in-memory or elsewhere data derived from us

*/

// TODO: Another big deal for the client and the server will be the Nagle packet flushing optimization




// In our RPC, these will contain a serialized ServerDescriptor representing which server is sending the request and who is the designated receiver
// NOTE: All key names must be lowercase as they may get normalized in http2 transport anyway and case may not get preversed on the other side
const FromKey: &str = "from";
const ToKey: &str = "to";
const ClusterIdKey: &str = "cluster-id";


pub type Metadata = HashMap<String, String>;


pub fn unmarshal<'de, T>(data: &[u8]) -> Result<T> where T: Deserialize<'de> {
	let mut de = rmps::Deserializer::new(data);
	Deserialize::deserialize(&mut de).map_err(|e| "Failed to parse data".into())
}

pub fn marshal<T>(obj: T) -> Result<Vec<u8>> where T: Serialize {
	let mut buf = Vec::new();
	obj.serialize(&mut rmps::Serializer::new_named(&mut buf))
		.map_err(|_| Error::from("Failed to serialize data"))?;
	Ok(buf)
}

// Probably to be pushed out of here
pub struct ServerConfig<S> {
	pub inst: S,
	pub agent: NetworkAgentHandle
}

pub type ServerHandle<S> = Arc<ServerConfig<S>>;

type HttpClient = Mutex<hyper::Client<hyper::client::HttpConnector, hyper::Body>>;
type HttpResponse = hyper::Response<hyper::Body>;


pub type ServiceFuture<T> = Box<Future<Item=T, Error=Error> + Send>;

/// Internal RPC Server between servers participating in the consensus protocol
pub trait ServerService {

	fn client(&self) -> &Client;

	fn pre_vote(&self, req: RequestVoteRequest) -> ServiceFuture<RequestVoteResponse>;

	fn request_vote(&self, req: RequestVoteRequest) -> ServiceFuture<RequestVoteResponse>;
	
	fn append_entries(&self, req: AppendEntriesRequest) -> ServiceFuture<AppendEntriesResponse>;
	
	fn timeout_now(&self, req: TimeoutNow) -> ServiceFuture<()>;

	// Also InstallSnapshot

	/// This is the odd ball out internal client method
	/// NOTE: 'AddServer' and 'RemoveServer' will be implemented by clients in terms of this method
	fn propose(&self, req: ProposeRequest) -> ServiceFuture<ProposeResponse>;
}

fn bad_request() -> HttpResponse {
	hyper::Response::builder()
		.status(hyper::StatusCode::BAD_REQUEST)
		.body(hyper::Body::empty())
		.unwrap()
}

pub fn bad_request_because(text: &'static str) -> HttpResponse {
	text_response(hyper::StatusCode::BAD_REQUEST, text)
}


pub fn text_response(code: hyper::StatusCode, text: &'static str) -> HttpResponse {
	hyper::Response::builder()
		.status(code)
		.header("Content-Type", "text/plain; charset=utf-8")
		.body(Body::from(text))
		.unwrap()
}

fn rpc_response<Res>(res: Result<Res>) -> HttpResponse
	where Res: Serialize
{
	match res {
		Ok(r) => {
			let data = marshal(r).expect("Failed to serialize RPC response");

			hyper::Response::builder()
				.status(200)
				.body(hyper::Body::from(data))
				.unwrap()
		},
		Err(e) => {
			eprintln!("{:?}", e);
			
			hyper::Response::builder()
				.status(500)
				.body(Body::empty())
				.unwrap()
		}
	}
}

fn run_handler<'a, S, F, Req, Res: 'static>(inst: &'a S, data: Bytes, f: F)
	-> impl Future<Item=hyper::Response<hyper::Body>, Error=hyper::Response<hyper::Body>> + Send
	where F: Fn(&S, Req) -> ServiceFuture<Res>,
		  Req: Deserialize<'a>,
		  Res: Serialize
{

	let start = move || -> FutureResult<_, hyper::Response<hyper::Body>> {
		let req: Req = match unmarshal(data.as_ref()) {
			Ok(v) => v,
			Err(_) => {
				eprintln!("Failed to parse RPC request");
				return err(bad_request());
			}
		};
		let res = f(inst, req);
		ok(res)
	};

	start().and_then(|res| {
		res.then(|r| {
			ok(rpc_response(r))
		})
	})
}


fn parse_metadata(headers: &hyper::HeaderMap) -> std::result::Result<Metadata, &'static str> {
	let mut meta = HashMap::new();
	for (k, v) in headers.iter() {
		// NOTE: We assume that this will always be in lowercase
		let name_raw = k.as_str();

		if name_raw.starts_with("x-") {
			let name = name_raw.split_at(2).1;
			let val = match v.to_str() {
				Ok(s) => s,
				Err(e) => return Err("Value is not a valid string")
			};
			meta.insert(name.to_owned(), val.to_owned());
		}
	}

	Ok(meta)
}

// Borrow<S> + 

// TODO: We could make it not arc if we can maintain some type of handler that definitely outlives the future being returned
// TODO: If we can provide a generic router, then we can abstract away the fact that it is for a ServerService
pub fn run_server<I: 'static, R, F: 'static>(port: u16, inst: I, router: &'static R) -> impl Future<Item=(), Error=()>
	where I: Clone + Send + Sync,
		  R: (Fn(hyper::Uri, Metadata, Bytes, I) -> F) + Send + Sync,
		  F: Future<Item=HttpResponse, Error=HttpResponse> + Send
{

	let addr = ([127, 0, 0, 1], port).into();

	let server = hyper::Server::bind(&addr)
		.serve(move || {
			let inst = inst.clone();

			hyper::service::service_fn(move |req: hyper::Request<hyper::Body>| {
				
				println!("GOT REQUEST {:?} {:?}", req.method(), req.uri());

				let inst = inst.clone();

				let f = lazy(move || {
					if req.method() != hyper::Method::POST {
						return err(bad_request());
					}

					let (parts, body) = req.into_parts();

					let mut meta = match parse_metadata(&parts.headers) {
						Ok(v) => v,
						Err(_) => return err(bad_request())
					};

					// TODO: Ideally filter requests based on metadata before the main data payload is read from the socket (especially for things like authentication)

					ok((parts.uri, meta, body))
				})
				.and_then(|(uri, meta, body)| {

					body
					.map_err(|e| {
						println!("{:?}", e);
						bad_request()
					})
					.fold(Vec::new(), |mut buf, c| -> FutureResult<Vec<_>, _> {
						buf.extend_from_slice(&c);
						ok(buf)
					})
					.and_then(move |buf| {
						ok((uri, meta, buf))
					})
				})
				.and_then(move |(uri, meta, data)| {
					router(uri, meta, data.into(), inst)
				});

				f.then(|res| -> FutureResult<HttpResponse, std::io::Error> {
					match res {
						Ok(v) => ok(v),
						Err(e) => ok(e)
					}
				})
			})
		})
		.map_err(|e| eprintln!("server error: {}", e));

	server
}

// TODO: Ideally the response type in the future needs be encapsulated so that we can manage a zero-copy deserialization
// The figure will resolve to an error only if there is a serious protocol/network error. Otherwise, regular error responses will be encoded in the inner result 
fn make_request_single<'a, Res>(client: &HttpClient, addr: &String, path: &'static str, meta: &Metadata, data: Bytes)
	-> impl Future<Item=(Metadata, Result<Res>), Error=Error>
	
	where Res: Deserialize<'a>	
{
	let mut b = hyper::Request::builder();

	b.uri(format!("{}{}", addr, path))
		.method("POST");
	
	for (k, v) in meta {
		// On the other side we will extract custom metadata given that it will have this format
		b.header(("x-".to_owned() + k).as_str(), v.as_str());
	}

	let r = b.body(Body::from(data))
		.expect("Failed to build RPC request");

	let req = {
		let c = client.lock().unwrap();
		c.request(r)
	};

	req
	.map_err(|e| e.into())
	.and_then(|resp| {

		let (parts, body) = resp.into_parts();

		let meta = match parse_metadata(&parts.headers) {
			Ok(v) => v,
			Err(_) => return Either::A(err("Invalid metadata in RPC response".into()))
		};

		if !parts.status.is_success() {
			return Either::A(ok(
				(
					meta,
					Err(format!("RPC call failed with code: {}", parts.status.as_u16()).into())
				)
			));
		}
	
		Either::B(body
		.map_err(|e| e.into())
		.fold(Vec::new(), |mut buf, c| -> FutureResult<Vec<_>, Error> {
			buf.extend_from_slice(&c);
			ok(buf)
		})
		.and_then(|buf| {
			let ret = match unmarshal(&buf) {
				Ok(v) => v,
				Err(_) => return err("Failed to parse RPC response".into())
			};

			futures::future::ok((meta, Ok(ret)))
		}))
	})
}

// TODO: We will eventually wrap these in an client struct that maintains a nice persistent connection (will also need to negotiate proper the right cluster_id and server_id on both ends for the connection to be opened)


pub fn DiscoverService_router<R: 'static, S: 'static>(
	uri: &hyper::Uri,
	meta: &Metadata,
	data: &Bytes,
	inst: &R

) -> std::result::Result<Option<HttpResponse>, HttpResponse>
	where R: Borrow<S> + Clone + Send + Sync,
		  S: ServerService + Send + Sync
{

	let mut agent = inst.borrow().client().agent().lock().unwrap();

	let our_cluster_id = agent.cluster_id.unwrap();

	// We first validate the cluster id because it must be valid for us to trust any of the other routing data
	let cluster_validated = if let Some(h) = meta.get(ClusterIdKey) {
		let cid = h.parse::<ClusterId>()
			.map_err(|_| bad_request_because("Invalid cluster id"))?;

		if cid != our_cluster_id {
			// TODO: This is a good reason to send back our cluster_id so that they can delete us as a route
			return Err(bad_request_because("Mismatching cluster id"));
		}

		true
	} else {
		false
	};

	// Record who sent us this message
	// TODO: Should receiving a message from one's self be an error?
	if let Some(h) = meta.get(FromKey) {
		if !cluster_validated {
			return Err(bad_request_because("Received From header without a cluster id check"));
		}

		let desc = ServerDescriptor::parse(h).map_err(|s| bad_request_because(s))?;
		agent.add_route(desc);
	}

	// Verify that we are the intended recipient of this message
	let to_verified = if let Some(h) = meta.get(ToKey) {
		if !cluster_validated {
			return Err(bad_request_because("Received To header without a cluster id check"));
		}

		let addr = ServerDescriptor::parse(h).map_err(|s| bad_request_because(s))?;
		let our_ident = agent.identity.as_ref().unwrap();

		if addr.id != our_ident.id {
			// Bail out. The client should adjust its routing info based on the identity we return back here
			return Err(hyper::Response::builder()
				.status(hyper::StatusCode::BAD_REQUEST)
				.header("Content-Type", "text/plain; charset=utf-8")
				.header(("x-".to_owned() + FromKey).as_str(), our_ident.to_string())
				.body(Body::from("Not the intended recipient"))
				.unwrap())
		}

		true
	}
	else {
		false
	};


	let mut res = match uri.path() {
		"/DiscoveryService/Announce" => {

			// TODO: Ideally don't hold the agent locked while deserializing
			let req: Announcement = match unmarshal(data.as_ref()) {
				Ok(v) => v,
				Err(_) => {
					eprintln!("Failed to parse RPC request");
					return Ok(Some(bad_request()));
				}
			};

			// When not verified, announcements can only be annonymous
			if req.routes.len() > 0 && !cluster_validated {
				eprintln!("Receiving new routes from a non-verified server");
				return Ok(Some(bad_request()));
			}

			agent.apply(&req);

			rpc_response(Ok(agent.serialize()))
		},
		// Does not match, so fallthrough to the regular routes
		_ => {
			// All other services depend on reliable ids, so we will block anything that is not well validated
			if !cluster_validated || !to_verified {
				return Err(bad_request());
			}

			return Ok(None);
		}
	};

	if !cluster_validated {
		res.headers_mut().insert(
			hyper::header::HeaderName::from_str(&("x-".to_owned() + ClusterIdKey)).unwrap(),
			hyper::header::HeaderValue::from_str(&agent.cluster_id.unwrap().to_string()).unwrap()
		);
	}

	if !to_verified {
		let our_ident = agent.identity.as_ref().unwrap();

		// TODO: This is basically redundant with the line that we have further above
		res.headers_mut().insert(
			hyper::header::HeaderName::from_str(&("x-".to_owned() + FromKey)).unwrap(),
			hyper::header::HeaderValue::from_str(our_ident.to_string().as_str()).unwrap()
		);
	}

	Ok(Some(res))
}

#[async]
pub fn ServerService_router<R: 'static, S: 'static>(
	uri: hyper::Uri,
	meta: Metadata,
	data: Bytes,
	inst: R

) -> std::result::Result<HttpResponse, HttpResponse>
	where R: Borrow<S> + Clone + Send + Sync,
		  S: ServerService + Send + Sync
{

	if let Some(res) = DiscoverService_router(&uri, &meta, &data, &inst)? {
		return Ok(res);
	}

	// XXX: If we can generalize this further, then this is the only thing that would need to be templated in order to run the server (i.e. for different types of rpcs)
	match uri.path() {
		"/ConsensusService/PreVote" => {
			await!(run_handler(inst.borrow(), data, S::pre_vote))
		},
		"/ConsensusService/RequestVote" => {
			await!(run_handler(inst.borrow(), data, S::request_vote))
		},
		"/ConsensusService/AppendEntries" => {
			await!(run_handler(inst.borrow(), data, S::append_entries))
		},
		"/ConsensusService/Propose" => {
			await!(run_handler(inst.borrow(), data, S::propose))
		},
		"/ConsensusService/TimeoutNow" => {
			await!(run_handler(inst.borrow(), data, S::timeout_now))
		},
		_ => {
			Err(bad_request())
		}
	}
}

pub enum To<'a> {
	Addr(&'a String),
	Id(ServerId)
}

pub struct Client {
	inner: HttpClient,
	agent: NetworkAgentHandle
}

impl Client {
	pub fn new(agent: NetworkAgentHandle) -> Self {
		let c = hyper::Client::builder()
			// We don't use the hostname for anything
			.set_host(false)
			// Our servers will always 
			.http2_only(true)
			.keep_alive(true)
			// NOTE: The default would also work. This can be anything that is larger than the heartbeat timer for leaders 
			.keep_alive_timeout(std::time::Duration::from_secs(30))
			.build_http();

		Client {
			inner: Mutex::new(c),
			agent
		}
	}

	pub fn agent(&self) -> &Mutex<NetworkAgent> {
		self.agent.as_ref()
	}

	// TODO: Must support a mode that batches sends to many servers all in one (while still allowing each individual promise to be externally controlled)
	// TODO: Also optimizable is ensuring that the metadata is only ever seralize once
	fn make_request<'a, 'b, Req, Res>(&self, to: To<'b>, path: &'static str, req: &Req)
		-> impl Future<Item=Res, Error=Error>
		where Req: Serialize,
			Res: Deserialize<'a>	
	{

		let mut meta: Metadata = HashMap::new();
		
		let mut agent = self.agent.lock().unwrap();
		if let Some(c) = agent.cluster_id {
			meta.insert(ClusterIdKey.to_owned(), c.to_string());
		}

		if let Some(ref id) = agent.identity {
			meta.insert(FromKey.to_owned(), id.to_string());
		}
		
		let addr = match to {
			To::Addr(s) => s.clone(),
			To::Id(id) => {
				if let Some(e) = agent.lookup(id) {
					meta.insert(ToKey.to_owned(), e.desc.to_string());
					e.desc.addr.clone()
				}
				else {
					println!("MISS {}", id);

					// TODO: then this is a good incentive to ask some other server for a new list of server addrs immediately (if we are able to communicate with out discovery service)
					return Either::A(err("No route for specified server id".into()))
				}
			}
		};

		drop(agent);


		let data = marshal(req).expect("Failed to serialize RPC request");

		let agent_handle = self.agent.clone();

		Either::B(make_request_single(&self.inner, &addr, path, &meta, data.into())
		.and_then(move |(meta, res)| {

			let mut agent = agent_handle.lock().unwrap();
			
			if let Some(v) = meta.get(ClusterIdKey) {
				let cid_given = v.parse::<u64>().unwrap();

				if let Some(cid) = agent.cluster_id {
					if cid != cid_given {
						return err("Received response with mismatching cluster_id".into())
					}
				}
				else {
					agent.cluster_id = Some(cid_given);
				}
			}

			if let Some(v) = meta.get(FromKey) {
				let desc = match ServerDescriptor::parse(v) {
					Ok(v) => v,
					Err(_) => return err("Invalid from metadata received".into())
				};

				// TODO: If we originally requested this server under a different id, it would be nice to erase that other record or tombstone it

				agent.add_route(desc);
			}

			res.into()
		}))
	}

	pub fn call_pre_vote(&self, to: ServerId, req: &RequestVoteRequest)
		-> impl Future<Item=RequestVoteResponse, Error=Error> {

		self.make_request(To::Id(to), "/ConsensusService/PreVote", req)
	}

	pub fn call_request_vote(&self, to: ServerId, req: &RequestVoteRequest)
		-> impl Future<Item=RequestVoteResponse, Error=Error> {

		self.make_request(To::Id(to), "/ConsensusService/RequestVote", req)
	}

	pub fn call_append_entries(&self, to: ServerId, req: &AppendEntriesRequest)
		-> impl Future<Item=AppendEntriesResponse, Error=Error> {

		self.make_request(To::Id(to), "/ConsensusService/AppendEntries", req)
	}

	pub fn call_propose(&self, to: ServerId, req: &ProposeRequest)
		-> impl Future<Item=ProposeResponse, Error=Error> {

		self.make_request(To::Id(to), "/ConsensusService/Propose", req)
	}

	// TODO: In general, we can always just send up our current list because the contents as pretty trivial
	// TODO: When normal clients are connecting, this may be a bit expensive of an rpc to call if it gets called many times to exchange leadership information (the ideal case is to always only exchange the bare minimum number of routes that the client needs to know about to get to the leaders it needs / knows about)
	
	// TODO: Probably the simplest improvement to this is to only ever broadcast changes that we've seen since we've last successfully replciated to this server
	// Basically a DynamoDB style replicated log per server that eventually replicates all of its changes to all other servers

	/// Used for sharing server discovery
	/// The request payload is a list of routes known to the requesting server. The response is the list of all routes on the receiving server after this request has been processed
	/// 
	/// The internal implementation essentially will cause the sets of routes on both servers to converge to the same set after the request has suceeded
	pub fn call_announce(&self, to: To)
		-> impl Future<Item=Announcement, Error=Error> {
		

		let agent_handle = self.agent.clone();

		// TODO: Eventually it would probably be more efficient to pass in a single copy of the req for all of the servers that we want to announce to
		let req = {
			let agent = agent_handle.lock().unwrap();
			agent.serialize()
		};

		self.make_request(to, "/DiscoveryService/Announce", &req)
		.and_then(move |ann| {

			let mut agent = agent_handle.lock().unwrap();
			agent.apply(&ann);
			ok(ann)
		})
	}

}


