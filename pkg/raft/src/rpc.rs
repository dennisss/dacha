use super::errors::*;
use super::protos::*;
use futures::future::*;
use futures::{Future, Stream};
use futures::prelude::*;
use futures::prelude::await;
use hyper::{Body};
use serde::{Deserialize, Serialize};
use std::sync::{Arc};
use bytes::Bytes;
use std::borrow::Borrow;

/*
	Helpers for the RPC calls going between servers in the raft group

	Not for handling external interfaces
*/



// Standard serializer/deserializer we will use for most things

pub fn unmarshal<'de, T>(data: Bytes) -> Result<T> where T: Deserialize<'de> {
	let mut de = rmps::Deserializer::new(&data[..]);
	Deserialize::deserialize(&mut de).map_err(|e| "Failed to parse data".into())
}

pub fn marshal<T>(obj: T) -> Result<Bytes> where T: Serialize {
	let mut buf = Vec::new();
	obj.serialize(&mut rmps::Serializer::new(&mut buf))
		.map_err(|_| Error::from("Failed to serialize data"))?;
	Ok(bytes::Bytes::from(buf))
}


/*

// But yes, if we just assume that all routes are trivial, then there isn't really much of a point to any of this stuff

// The realistic best model is to create a server builder

pub struct ServiceDefinition<S> {
	
	pub methods: HashMap<&'static str, Fn(&mut S, Vec<u8>) -> Result<Vec<u8>>


	// Every service definition is a map from taking bytes as input and returning some bytes as output (but naturally it is also bound with a specific )

}

// Naturally this only really helps us for the request end
// The response end will be completely different

fn new_consensus_service() -> ServiceDefinition> {

	let mut s = ServiceDefinition {
		methods: HashMap::new()
	};

	s.methods.insert("/ConsensusService/RequestVote", |s, data| {
		// Naturally we would probably bubble 

		run_handler(inst_handle, data, S::request_vote)
	})

	/*

	 => {
		run_handler(inst_handle, data, S::request_vote)
	},
	"/ConsensusService/AppendEntries" => {
		run_handler(inst_handle, data, S::append_entries)
	},

	s.insert("")

	*/

	s


}

// But yes, we can register 

impl Server {



}


// Basically yes, 

*/

pub type ServiceFuture<T> = Box<Future<Item=T, Error=Error> + Send + 'static>;

/// Internal RPC Server between servers participating in the consensus protocol
pub trait ServerService {
	fn pre_vote(&self, req: RequestVoteRequest) -> ServiceFuture<RequestVoteResponse>;

	fn request_vote(&self, req: RequestVoteRequest) -> ServiceFuture<RequestVoteResponse>;
	
	fn append_entries(&self, req: AppendEntriesRequest) -> ServiceFuture<AppendEntriesResponse>;
	
	fn timeout_now(&self, req: TimeoutNow) -> ServiceFuture<()>;

	// This is the odd ball out internal client method
	fn propose(&self, req: ProposeRequest) -> ServiceFuture<ProposeResponse>;

	// Also InstallSnapshot, AddServer, RemoveServer
}

fn bad_request() -> hyper::Response<hyper::Body> {
	hyper::Response::builder()
		.status(hyper::StatusCode::BAD_REQUEST)
		.body(hyper::Body::empty())
		.expect("Failed to build bad request")
}

/*
	Basically given a path, we can register any meaningful service

	- Ideally we would implement some form of state machine thing to match between many 

*/

fn rpc_response<Res>(res: Result<Res>) -> hyper::Response<hyper::Body>
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

fn run_handler<'a, S, F, Req, Res: 'static>(inst: &'a S, data: Vec<u8>, f: F)
	-> Box<Future<Item=hyper::Response<hyper::Body>, Error=hyper::Response<hyper::Body>> + Send>
	where S: ServerService,
		  F: Fn(&S, Req) -> ServiceFuture<Res>,
		  Req: Deserialize<'a>,
		  Res: Serialize
{

	let start = move || -> FutureResult<_, hyper::Response<hyper::Body>> {
		let req: Req = match unmarshal(data.into()) {
			Ok(v) => v,
			Err(_) => {
				eprintln!("Failed to parse RPC request");
				return err(bad_request());
			}
		};
		let res = f(inst, req);
		ok(res)
	};

	Box::new(start().and_then(|res| {
		res.then(|r| {
			ok(rpc_response(r))
		})
	}))
}


// TODO: We could make it not arc if we can maintain some type of handler that definitely outlives the future being returned

pub fn run_server<R: 'static, S: 'static>(port: u16, inst: R) -> impl Future<Item=(), Error=()>
	where R: Borrow<S> + Clone + Send + Sync,
		  S: ServerService + Send + Sync
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

					ok(req)
				})
				.and_then(|req| {

					let (parts, body) = req.into_parts();

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
						ok((parts, buf))
					})
				})
				.and_then(move |(parts, data)| {

					let r = inst.borrow();
			
					// XXX: If we can generalize this further, then this is the only thing that would need to be templated in order to run the server (i.e. for different types of rpcs)
					match parts.uri.path() {
						"/ConsensusService/PreVote" => {
							run_handler(r, data, S::pre_vote)
						},
						"/ConsensusService/RequestVote" => {
							run_handler(r, data, S::request_vote)
						},
						"/ConsensusService/AppendEntries" => {
							run_handler(r, data, S::append_entries)
						},
						"/ConsensusService/Propose" => {
							run_handler(r, data, S::propose)
						},
						"/ConsensusService/TimeoutNow" => {
							run_handler(r, data, S::timeout_now)
						},
						_ => {
							Box::new(err(bad_request()))
						}
					}
				});


				f.then(|res| -> FutureResult<hyper::Response<Body>, std::io::Error> {
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


// TODO: Must support a mode that batches sends to many servers all in one (while still allowing each individual promise to be externally controlled)
fn make_request<'a, Req, Res>(addr: &String, path: &'static str, req: &Req)
	-> impl Future<Item=Res, Error=Error>
	where Req: Serialize,
		  Res: Deserialize<'a>	
{
	let data = marshal(req).expect("Failed to serialize RPC request");
	make_request_single(addr, path, data)
}

// TODO: Ideally the response type in the future needs be encapsulated so that we can manage a zero-copy deserialization
fn make_request_single<'a, Res>(addr: &String, path: &'static str, data: bytes::Bytes)
	-> impl Future<Item=Res, Error=Error>
	where Res: Deserialize<'a>	
{
	let client = hyper::Client::new();

	let r = hyper::Request::builder()
		.uri(format!("{}{}", addr, path))
		.method("POST")
		.body(Body::from(data))
		.expect("Failed to build RPC request");

	client
	.request(r)
	.map_err(|e| e.into())
	.and_then(|resp| {

		if !resp.status().is_success() {
			// NOTE: We assume that errors don't appear in the body
			return err(format!("RPC call failed with code: {}", resp.status().as_u16()).into());
		}

		ok(resp)
	})
	.and_then(|resp| {
		resp.into_body()
		.map_err(|e| e.into())
		.fold(Vec::new(), |mut buf, c| -> FutureResult<Vec<_>, Error> {
			buf.extend_from_slice(&c);
			ok(buf)
		})
		.and_then(|buf| {
			let ret = match unmarshal(buf.into()) {
				Ok(v) => v,
				Err(_) => return err("Failed to parse RPC response".into())
			};

			// XXX: We really want to be able to encapsulate this in a zero copy 
			futures::future::ok(ret)
		})
	})
}

// TODO: We will eventually wrap these in an client struct that maintains a nice persistent connection (will also need to negotiate proper the right cluster_id and server_id on both ends for the connection to be opened)



pub fn call_pre_vote(addr: &String, req: &RequestVoteRequest)
	-> impl Future<Item=RequestVoteResponse, Error=Error> {

	make_request(addr, "/ConsensusService/PreVote", req)
}

pub fn call_request_vote(addr: &String, req: &RequestVoteRequest)
	-> impl Future<Item=RequestVoteResponse, Error=Error> {

	make_request(addr, "/ConsensusService/RequestVote", req)
}

pub fn call_append_entries(addr: &String, req: &AppendEntriesRequest)
	-> impl Future<Item=AppendEntriesResponse, Error=Error> {

	make_request(addr, "/ConsensusService/AppendEntries", req)
}

pub fn call_propose(addr: &String, req: &ProposeRequest)
	-> impl Future<Item=ProposeResponse, Error=Error> {

	make_request(addr, "/ConsensusService/Propose", req)
}

