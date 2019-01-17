use super::errors::*;
use super::protos::*;
use futures::future::*;
use futures::{Future, Stream};
use hyper::{Body};
use serde::{Deserialize, Serialize};
use rmps::{Deserializer, Serializer};
use std::sync::{Arc, Mutex};


/*
	Helpers for the RPC calls going between servers in the raft group

	Not for handling external interfaces
*/


pub trait Server {
	fn request_vote(&mut self, req: RequestVoteRequest) -> Result<RequestVoteResponse>;
	fn append_entries(&mut self, req: AppendEntriesRequest) -> Result<AppendEntriesResponse>;
}

fn bad_request() -> hyper::Response<hyper::Body> {
	hyper::Response::builder()
		.status(hyper::StatusCode::BAD_REQUEST)
		.body(hyper::Body::empty())
		.unwrap()
}

pub fn run_server<S: 'static>(port: u16, inst_handle: Arc<Mutex<S>>)
	where S: Server + Send
 {
	let addr = ([127, 0, 0, 1], port).into();

	let server = hyper::Server::bind(&addr)
		.serve(move || {
			let inst_handle = inst_handle.clone();

			hyper::service::service_fn(move |req: hyper::Request<hyper::Body>| {
				
				let inst_handle = inst_handle.clone();

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

					match parts.uri.path() {
						"/request_vote" => {
							// Parse the request and execute stuff

							let mut de = Deserializer::new(&data[..]);
							let ret: RequestVoteRequest = Deserialize::deserialize(&mut de).unwrap();
							
							let mut inst = inst_handle.lock().unwrap();

							inst.request_vote(ret);

							ok(bad_request())
						},
						"/append_entries" => {
							ok(bad_request())
						},
						_ => {
							err(bad_request())
						}
					}

					//ok(hyper::Response::new(hyper::Body::empty()))
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

	hyper::rt::run(server);
}


// TODO: Must support a mode that batches sends to many servers all in one (while still allowing each individual promise to be externally controlled)
fn make_request<'a, Req, Res>(server: &ServerDescriptor, path: &'static str, req: &Req)
	-> impl Future<Item=Res, Error=Error>
	where Req: Serialize,
		  Res: Deserialize<'a>	
{

	let data = {
		let mut buf = Vec::new();
		req.serialize(&mut Serializer::new(&mut buf)).unwrap();
		bytes::Bytes::from(buf)
	};

	make_request_single(server, path, data)
}

// TODO: Ideally the response type in the future needs be encapsulated so that we can manage a zero-copy deserialization
fn make_request_single<'a, Res>(server: &ServerDescriptor, path: &'static str, data: bytes::Bytes)
	-> impl Future<Item=Res, Error=Error>
	where Res: Deserialize<'a>	
{
	let client = hyper::Client::new();

	let r = hyper::Request::builder()
		.uri(format!("{}{}", server.addr, path))
		.body(Body::from(data))
		.unwrap();

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
			let ret = Deserialize::deserialize(&mut de).unwrap();

			// XXX: We really want to be able to encapsulate this in a zero copy 
			futures::future::ok(ret)
		})
	})
}

// TODO: We will eventually wrap these in an client struct that maintains a nice persistent connection

pub fn call_request_vote(server: &ServerDescriptor, req: &RequestVoteRequest)
	-> impl Future<Item=RequestVoteResponse, Error=Error> {

	make_request(server, "/request_vote", req)
}

pub fn call_append_entries(server: &ServerDescriptor, req: &AppendEntriesRequest)
	-> impl Future<Item=AppendEntriesResponse, Error=Error> {

	make_request(server, "/append_entries", req)
}

