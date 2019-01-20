use super::errors::*;
use super::protos::*;
use futures::future::*;
use futures::{Future, Stream};
use hyper::{Body};
use serde::{Deserialize, Serialize};
use rmps::{Deserializer, Serializer};
use std::sync::{Arc};

/*
	Helpers for the RPC calls going between servers in the raft group

	Not for handling external interfaces
*/


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


/// Internal RPC Server between servers participating in the consensus protocol
pub trait ServerService {
	fn pre_vote(&self, req: RequestVoteRequest) -> Result<RequestVoteResponse>;

	fn request_vote(&self, req: RequestVoteRequest) -> Result<RequestVoteResponse>;
	
	fn append_entries(&self, req: AppendEntriesRequest) -> Result<AppendEntriesResponse>;
	
	fn timeout_now(&self, req: TimeoutNow) -> Result<()>;

	// This is the odd ball out internal client method
	fn propose(&self, req: ProposeRequest) -> Result<ProposeResponse>;

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
			let data = {
				let mut buf = Vec::new();
				r.serialize(&mut Serializer::new(&mut buf)).expect("Failed to serialize RPC request");
				bytes::Bytes::from(buf)
			};

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

fn run_handler<'a, S, F, Req, Res>(inst: Arc<S>, data: Vec<u8>, f: F)
	-> FutureResult<hyper::Response<hyper::Body>, hyper::Response<hyper::Body>>
	where S: ServerService,
		  F: Fn(&S, Req) -> Result<Res>,
		  Req: Deserialize<'a>,
		  Res: Serialize
{

	let mut de = Deserializer::new(&data[..]);
	let req: Req = Deserialize::deserialize(&mut de).expect("Failed to parse request");
	
	//let mut inst = inst_handle.lock().expect("Failed to lock instance");

	let res = f(&inst, req);

	ok(rpc_response(res))
}


pub fn run_server<S: 'static>(port: u16, inst: Arc<S>) -> impl Future<Item=(), Error=()>
	where S: ServerService + Send + Sync
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

					// We would 

					// XXX: If we can generalize this further, then this is the only thing that would need to be templated in order to run the server (i.e. for different types of rpcs)
					match parts.uri.path() {
						"/ConsensusService/PreVote" => {
							run_handler(inst, data, S::pre_vote)
						},
						"/ConsensusService/RequestVote" => {
							run_handler(inst, data, S::request_vote)
						},
						"/ConsensusService/AppendEntries" => {
							run_handler(inst, data, S::append_entries)
						},
						"/ConsensusService/Propose" => {
							run_handler(inst, data, S::propose)
						},
						"/ConsensusService/TimeoutNow" => {
							run_handler(inst, data, S::timeout_now)
						},
						_ => {
							err(bad_request())
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

	let data = {
		let mut buf = Vec::new();
		req.serialize(&mut Serializer::new(&mut buf)).expect("Failed to serialize RPC request");
		bytes::Bytes::from(buf)
	};

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
			let mut de = Deserializer::new(&buf[..]);
			let ret = match Deserialize::deserialize(&mut de) {
				Ok(v) => v,
				Err(e) => return err("Failed to parse RPC response".into())
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

