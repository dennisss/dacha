use serde::{Deserialize, Serialize};
use bytes::Bytes;
use std::borrow::Borrow;
use std::collections::HashMap;
use std::sync::Arc;
use common::async_std::sync::Mutex;
use std::time::SystemTime;
use std::str::FromStr;
use common::errors::*;
use http::spec::{RequestBuilder, ResponseBuilder, Method, HttpHeader};
use http::body::*;
use http::status_code;
use http::header;
use super::protos::*;
use super::routing::*;

/*
	Helpers for making an RPC server for communication between machines
	- Similar to gRPC, we currently support metadata key-value pairs that are
	  added to a request
	- But, we also support metadata to be returned in the response as well
	  separately from the return value

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

	- What we want to avoid is the fourth trivial state that is an
	  identity/or/routes without a cluster_id
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
	Deserialize::deserialize(&mut de)
		.map_err(|e| err_msg("Failed to parse data"))
}

pub fn marshal<T>(obj: T) -> Result<Vec<u8>> where T: Serialize {
	let mut buf = Vec::new();
	obj.serialize(&mut rmps::Serializer::new_named(&mut buf))
		.map_err(|_| err_msg("Failed to serialize data"))?;
	Ok(buf)
}

// Probably to be pushed out of here
pub struct ServerConfig<S> {
	pub inst: S,
	pub agent: NetworkAgentHandle
}

pub type ServerHandle<S> = Arc<ServerConfig<S>>;

type HttpClient = http::client::Client;
type HttpResponse = http::spec::Response;


/// Internal RPC Server between servers participating in the consensus protocol
#[async_trait]
pub trait ServerService {
	fn client(&self) -> &Client;

	async fn pre_vote(&self, req: RequestVoteRequest)
		-> Result<RequestVoteResponse>;

	async fn request_vote(&self, req: RequestVoteRequest)
		-> Result<RequestVoteResponse>;
	
	async fn append_entries(&self, req: AppendEntriesRequest)
		-> Result<AppendEntriesResponse>;
	
	async fn timeout_now(&self, req: TimeoutNow) -> Result<()>;

	// Also InstallSnapshot

	/// This is the odd ball out internal client method
	/// NOTE: 'AddServer' and 'RemoveServer' will be implemented by clients in terms of this method
	async fn propose(&self, req: ProposeRequest) -> Result<ProposeResponse>;
}

fn bad_request() -> HttpResponse {
	ResponseBuilder::new()
		.status(status_code::BAD_REQUEST)
		.body(EmptyBody())
		.build().unwrap()
}

pub fn bad_request_because(text: &'static str) -> HttpResponse {
	text_response(status_code::BAD_REQUEST, text)
}


pub fn text_response(code: status_code::StatusCode,
					 text: &'static str) -> HttpResponse {
	ResponseBuilder::new()
		.status(code)
		.header(header::CONTENT_TYPE, "text/plain; charset=utf-8")
		.body(BodyFromData(text.as_bytes()))
		.build().unwrap()
}

fn rpc_response<Res>(res: Result<Res>) -> HttpResponse where Res: Serialize {
	match res {
		Ok(r) => {
			let data = marshal(r).expect("Failed to serialize RPC response");
			ResponseBuilder::new()
				.status(status_code::OK)
				.body(EmptyBody())
				.build().unwrap()
		},
		Err(e) => {
			eprintln!("{:?}", e);
			ResponseBuilder::new()
				.status(status_code::INTERNAL_SERVER_ERROR)
				.body(EmptyBody())
				.build().unwrap()
		}
	}
}

use std::pin::Pin;
use std::future::Future;
use common::futures::TryFutureExt;

/*
error[E0271]: type mismatch resolving `


for<'b> <fn(&S, protos::AppendEntriesRequest) -> std::pin::Pin<std::boxed::Box<dyn std::future::Future<Output = std::result::Result<protos::AppendEntriesResponse, common::errors::Error>> + std::marker::Send>>

{<S as rpc::ServerService>::append_entries::<'_, '_>} as std::ops::FnOnce<(&'b S, _)>>::Output == std::pin::Pin<std::boxed::Box<(dyn std::future::Future<Output = std::result::Result<_, common::errors::Error>> + std::marker::Send + 'b)>>`




fn propose<'life0, 'async_trait>(
            &'life0 self,
            req: ProposeRequest,
        ) -> ::core::pin::Pin<
            Box<
                dyn ::core::future::Future<Output = Result<ProposeResponse>>
                    + ::core::marker::Send
                    + 'async_trait,
            >,
        >
        where
            'life0: 'async_trait,
            Self: 'async_trait;
*/

use common::async_fn::AsyncFnOnce2;


async fn run_handler<'a, S: 'static, Req, Res: 'static, F>(
	inst: &S, data: Bytes, f: F)
	-> std::result::Result<HttpResponse, HttpResponse>
	where for<'b> F: AsyncFnOnce2<&'b S, Req, Output=Result<Res>>,
		  Req: Deserialize<'a> + 'static,
		  Res: Serialize + 'static {
	let req: Req = match unmarshal(data.as_ref()) {
		Ok(v) => v,
		Err(_) => {
			eprintln!("Failed to parse RPC request");
			return Err(bad_request());
		}
	};

	let res = f.call_once(inst, req).await;
	Ok(rpc_response(res))
}


fn parse_metadata(headers: &http::spec::HttpHeaders)
	-> std::result::Result<Metadata, &'static str> {
	let mut meta = HashMap::new();
	for h in headers.raw_headers.iter() {
		// NOTE: We assume that this will always be in lowercase
		let name_raw: &str = h.name.as_ref();

		if name_raw.starts_with("x-") {
			let name = name_raw.split_at(2).1;
			// TODO: Are header values allowed to contain non-string chars?
//			let val = match h.value v.to_str() {
//				Ok(s) => s,
//				Err(e) => return Err("Value is not a valid string")
//			};
			meta.insert(name.to_owned(), h.value.to_string());
		}
	}

	Ok(meta)
}

// Borrow<S> + 

// TODO: We could make it not arc if we can maintain some type of handler that
// definitely outlives the future being returned
// TODO: If we can provide a generic router, then we can abstract away the fact
// that it is for a ServerService
pub async fn run_server<I: 'static, R, F: 'static>(
	port: u16, inst: I, router: &'static R)
	where I: Clone + Send + Sync,
		  R: (Fn(http::uri::Uri, Metadata, Bytes, I) -> F) + Send + Sync,
		  F: std::future::Future<Output=std::result::Result<HttpResponse, HttpResponse>> + Send {

//	let addr = ([127, 0, 0, 1], port).into();

//	let server = http::server::HttpServer::new(port, )

	let service_fn = move |mut req: http::spec::Request| {
		let inst = inst.clone();
		println!("GOT REQUEST {:?} {:?}", req.head.method, req.head.uri);

		let f = async move || {
			if req.head.method != Method::POST {
				return bad_request();
			}

			let mut meta = match parse_metadata(&req.head.headers) {
				Ok(v) => v,
				Err(_) => return bad_request()
			};

			// TODO: Ideally filter requests based on metadata before
			// the main data payload is read from the socket (especially
			// for things like authentication)

			let mut buf = vec![];
			if let Err(e) = req.body.read_to_end(&mut buf).await {
				return bad_request();
			}

			let ret = router(req.head.uri, meta, buf.into(), inst).await;
			match ret {
				Ok(v) => v,
				Err(v) => v
			}
		};

		f()
	};

	http::server::HttpServer::new(port, http::server::HttpFn(service_fn))
		.run().await
		.map_err(|e| eprintln!("server error: {}", e)).ok();
}



// TODO: Ideally the response type in the future needs be encapsulated so that
// we can manage a zero-copy deserialization
// The figure will resolve to an error only if there is a serious
// protocol/network error. Otherwise, regular error responses will be encoded in
// the inner result
fn make_request_single<'a, 'b, Res: Send + Sync + Deserialize<'a> + 'static>(
	client: &'b http::client::Client, addr: &'b String, path: &'static str,
	meta: &'b Metadata, data: Bytes)
	-> impl Future<Output=Result<(Metadata, Result<Res>)>> + Send + 'b {
	async move {
		let mut b = http::spec::RequestBuilder::new();
		// TODO: Must split up into connection and path
		b = b.uri(format!("{}{}", addr, path))
			.method(Method::POST);

		for (k, v) in meta {
			// On the other side we will extract custom metadata given that it will
			// have this format
			b = b.header(format!("x-{}", k), v);
		}

		let r = b.body(BodyFromData(data)).build()
			.expect("Failed to build RPC request");

		let mut resp = client.request(r).await?;

		let meta2 = match parse_metadata(&resp.head.headers) {
			Ok(v) => v,
			Err(_) => return Err(err_msg("Invalid metadata in RPC response"))
		};

		if resp.head.status_code != status_code::OK {
			return Ok((
				meta2,
				Err(format_err!("RPC call failed with code: {}",
							resp.head.status_code.as_u16()).into())
			));
		}

		// TODO: Must check Content-Length
		let mut buf = vec![];
		resp.body.read_to_end(&mut buf).await?;

		let ret = match unmarshal(&buf) {
			Ok(v) => v,
			Err(_) => return Err(err_msg("Failed to parse RPC response"))
		};

		Ok((meta2, Ok(ret)))
	}
}

// TODO: We will eventually wrap these in an client struct that maintains a nice persistent connection (will also need to negotiate proper the right cluster_id and server_id on both ends for the connection to be opened)


pub async fn DiscoverService_router<R: 'static, S: 'static>(
	uri: &http::uri::Uri,
	meta: &Metadata,
	data: &Bytes,
	inst: &R
) -> std::result::Result<Option<HttpResponse>, HttpResponse>
	where R: Borrow<S> + Clone + Send + Sync,
		  S: ServerService + Send + Sync {
	let mut agent = inst.borrow().client().agent().lock().await;

	let our_cluster_id = agent.cluster_id.unwrap();

	// We first validate the cluster id because it must be valid for us to trust any of the other routing data
	let cluster_validated = if let Some(h) = meta.get(ClusterIdKey) {
		let cid = h.parse::<ClusterId>()
			.map_err(|_| bad_request_because("Invalid cluster id"))?;

		if cid != our_cluster_id {
			// TODO: This is a good reason to send back our cluster_id so that
			// they can delete us as a route
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
			return Err(bad_request_because(
				"Received From header without a cluster id check"));
		}

		let desc = ServerDescriptor::parse(h)
			.map_err(|s| bad_request_because(s))?;
		agent.add_route(desc);
	}

	// Verify that we are the intended recipient of this message
	let to_verified = if let Some(h) = meta.get(ToKey) {
		if !cluster_validated {
			return Err(bad_request_because(
				"Received To header without a cluster id check"));
		}

		let addr = ServerDescriptor::parse(h).map_err(|s| bad_request_because(s))?;
		let our_ident = agent.identity.as_ref().unwrap();

		if addr.id != our_ident.id {
			// Bail out. The client should adjust its routing info based on the
			// identity we return back here
			return Err(ResponseBuilder::new()
						   .status(status_code::BAD_REQUEST)
						   .header(header::CONTENT_TYPE, "text/plain; charset=utf-8")
						   .header(format!("x-{}", FromKey), our_ident.to_string())
						   .body(BodyFromData(b"Not the intended recipient"))
						   .build().unwrap());
		}

		true
	}
	else {
		false
	};


	let mut res = match uri.path.as_str() {
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
			// All other services depend on reliable ids, so we will block
			// anything that is not well validated
			if !cluster_validated || !to_verified {
				return Err(bad_request());
			}

			return Ok(None);
		}
	};

	if !cluster_validated {
		res.head.headers.raw_headers.push(HttpHeader::new(
			format!("x-{}", ClusterIdKey), agent.cluster_id.unwrap().to_string()
		));
	}

	if !to_verified {
		let our_ident = agent.identity.as_ref().unwrap();

		// TODO: This is basically redundant with the line that we have further above
		res.head.headers.raw_headers.push(HttpHeader::new(
			format!("x-{}", FromKey), our_ident.to_string()
		));
	}

	Ok(Some(res))
}

async fn run_pre_vote<S: ServerService>(
	s: &S, r: crate::protos::RequestVoteRequest) -> Result<RequestVoteResponse> {
	s.pre_vote(r).await
}

async fn run_request_vote<S: ServerService>(
	s: &S, r: crate::protos::RequestVoteRequest) -> Result<RequestVoteResponse> {
	s.request_vote(r).await
}


async fn run_append_entries<S: ServerService>(
	s: &S, r: AppendEntriesRequest) -> Result<AppendEntriesResponse> {
	s.append_entries(r).await
}

async fn run_propose<S: ServerService>(
	s: &S, r: ProposeRequest) -> Result<ProposeResponse> {
	s.propose(r).await
}

pub async fn ServerService_router<R: 'static, S: 'static>(
	uri: http::uri::Uri,
	meta: Metadata,
	data: Bytes,
	inst: R

) -> std::result::Result<HttpResponse, HttpResponse>
	where R: Borrow<S> + Clone + Send + Sync,
		  S: ServerService + Send + Sync + 'static
{
	if let Some(res) = DiscoverService_router(&uri, &meta, &data, &inst).await? {
		return Ok(res);
	}

	// XXX: If we can generalize this further, then this is the only thing that
	// would need to be templated in order to run the server (i.e. for different
	// types of rpcs)
	match uri.path.as_str() {
		"/ConsensusService/PreVote" => {
			run_handler(inst.borrow(), data, run_pre_vote).await
		},
		"/ConsensusService/RequestVote" => {
			run_handler(inst.borrow(), data, run_request_vote).await
		},
		"/ConsensusService/AppendEntries" => {
			run_handler(inst.borrow(), data, run_append_entries).await
		},
		"/ConsensusService/Propose" => {
			run_handler(inst.borrow(), data, run_propose).await
		},
//		"/ConsensusService/TimeoutNow" => {
//			run_handler(inst.borrow(), data, S::timeout_now).await
//		},
		_ => {
			Err(bad_request())
		}
	}
}

/*
expected trait `std::future::Future<Output = std::result::Result<_, common::errors::Error>>`, found trait `std::future::Future<Output = std::result::Result<protos::ProposeResponse, common::errors::Error>> + std::marker::Send
*/


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
		// TODO: Hyper http clients only allow sending requests with a mutable
		// reference?

//		let c = hyper::Client::builder()
//			// We don't use the hostname for anything
//			.set_host(false)
//			// Our servers will always
//			.http2_only(true)
//			.keep_alive(true)
//			// NOTE: The default would also work. This can be anything that is larger than the heartbeat timer for leaders
//			.keep_alive_timeout(std::time::Duration::from_secs(30))
//			.build_http();

		// TODO
		let c = http::client::Client::create("").unwrap();

		Client {
			inner: c,
			agent
		}
	}

	pub fn agent(&self) -> &Mutex<NetworkAgent> {
		self.agent.as_ref()
	}

	// TODO: Must support a mode that batches sends to many servers all in one
	// (while still allowing each individual promise to be externally controlled)
	// TODO: Also optimizable is ensuring that the metadata is only ever
	// seralize once
	async fn make_request<'a, Req: 'static, Res: Send + Sync + 'static>(
		&'a self, to: To<'a>, path: &'static str, req: &'a Req) -> Result<Res>
		where Req: Serialize, Res: Deserialize<'a> {
		let mut meta: Metadata = HashMap::new();
		
		let mut agent = self.agent.lock().await;
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

					// TODO: then this is a good incentive to ask some other
					// server for a new list of server addrs immediately (if we
					// are able to communicate with out discovery service)
					return Err(err_msg("No route for specified server id"));
				}
			}
		};

		drop(agent);


		let data = marshal(req).expect("Failed to serialize RPC request");

		let agent_handle = self.agent.clone();

		let (meta, res) = make_request_single(
			&self.inner, &addr, path, &meta, data.into()).await?;

		let mut agent = agent_handle.lock().await;

		if let Some(v) = meta.get(ClusterIdKey) {
			let cid_given = v.parse::<u64>().unwrap();

			if let Some(cid) = agent.cluster_id {
				if cid != cid_given {
					return Err(err_msg(
						"Received response with mismatching cluster_id"));
				}
			}
			else {
				agent.cluster_id = Some(cid_given);
			}
		}

		if let Some(v) = meta.get(FromKey) {
			let desc = match ServerDescriptor::parse(v) {
				Ok(v) => v,
				Err(_) => return Err(err_msg("Invalid from metadata received"))
			};

			// TODO: If we originally requested this server under a
			// different id, it would be nice to erase that other record or
			// tombstone it

			agent.add_route(desc);
		}

		res
	}

	pub async fn call_pre_vote(&self, to: ServerId, req: &RequestVoteRequest)
		-> Result<RequestVoteResponse> {

		self.make_request(To::Id(to), "/ConsensusService/PreVote", req).await
	}

	pub async fn call_request_vote(&self, to: ServerId,
								   req: &RequestVoteRequest)
		-> Result<RequestVoteResponse> {
		self.make_request(
			To::Id(to), "/ConsensusService/RequestVote", req).await
	}

	pub async fn call_append_entries(
		&self, to: ServerId, req: &AppendEntriesRequest)
		-> Result<AppendEntriesResponse> {
		self.make_request(
			To::Id(to), "/ConsensusService/AppendEntries", req).await
	}

	pub async fn call_propose(&self, to: ServerId, req: &ProposeRequest)
		-> Result<ProposeResponse> {
		self.make_request(To::Id(to), "/ConsensusService/Propose", req).await
	}

	// TODO: In general, we can always just send up our current list because the
	// contents as pretty trivial
	// TODO: When normal clients are connecting, this may be a bit expensive of
	// an rpc to call if it gets called many times to exchange leadership
	// information (the ideal case is to always only exchange the bare minimum
	// number of routes that the client needs to know about to get to the
	// leaders it needs / knows about)
	
	// TODO: Probably the simplest improvement to this is to only ever broadcast
	// changes that we've seen since we've last successfully replciated to this
	// server
	// Basically a DynamoDB style replicated log per server that eventually
	// replicates all of its changes to all other servers

	/// Used for sharing server discovery
	/// The request payload is a list of routes known to the requesting server.
	/// The response is the list of all routes on the receiving server after
	/// this request has been processed
	/// 
	/// The internal implementation essentially will cause the sets of routes on
	/// both servers to converge to the same set after the request has suceeded
	pub async fn call_announce<'a>(&self, to: To<'a>) -> Result<Announcement> {
		let agent_handle = self.agent.clone();

		// TODO: Eventually it would probably be more efficient to pass in a
		// single copy of the req for all of the servers that we want to
		// announce to.
		let req = {
			let agent = agent_handle.lock().await;
			agent.serialize()
		};

		let ann = self.make_request(
			to, "/DiscoveryService/Announce", &req).await?;
		let mut agent = agent_handle.lock().await;
		agent.apply(&ann);
		Ok(ann)
	}

}


