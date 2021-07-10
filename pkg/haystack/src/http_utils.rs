use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::{Arc, Mutex};

use common::errors::*;
use protobuf_json::MessageJsonSerialize;
use common::async_fn::AsyncFn2;

// TODO: Need a better space for these shared helpers 

pub fn bad_request() -> http::Response {
	http::ResponseBuilder::new().status(http::status_code::BAD_REQUEST).build().unwrap()
}

pub fn invalid_method() -> http::Response {
	text_response(http::status_code::METHOD_NOT_ALLOWED, "Method not allowed")
}

pub fn bad_request_because(text: &'static str) -> http::Response {
	text_response(http::status_code::BAD_REQUEST, text)
}

pub fn json_response<M>(code: http::status_code::StatusCode, obj: &M) -> http::Response where M: protobuf::MessageReflection {
	let body = obj.serialize_json();
	
	// TODO: Perform response compression.

	http::ResponseBuilder::new()
		.status(code)
		.header("Content-Type", "application/json; charset=utf-8")
		.body(http::BodyFromData(body))
		.build()
		.unwrap()
}

pub fn text_response(code: http::status_code::StatusCode, text: &'static str) -> http::Response {
	http::ResponseBuilder::new()
		.status(code)
		.header("Content-Type", "text/plain; charset=utf-8")
		.body(http::BodyFromData(text))
		.build()
		.unwrap()
}

// 00
// 01
// 10
// 11

// 1/4

//0.5

// n=3
//  second dup or  (second different)*(third same)
// (1 / 1000) + (2 / 1000)*(999 / 1000) + (3 / 1000)(998 / 1000)
// Sum_i=(1..(k-1))( (i/n) * ((n - i + 1) / n) )


/// Wraps a regular async request in a wrapper that logs out errors and nicely responds to clients on errors
struct RequestHandlerWrap<Func, Arg> {
	func: Func,
	arg: Arc<Arg>
}

#[async_trait]
impl<
	Arg: Send + Sync,
	Func: 'static + Send + Sync + for<'a> AsyncFn2<http::Request, &'a Arg, Output=Result<http::Response>>
> http::RequestHandler for RequestHandlerWrap<Func, Arg> {
	async fn handle_request(&self, request: http::Request) -> http::Response {
		let method = request.head.method.clone();
		let uri = request.head.uri.clone();

		match self.func.call(request, self.arg.as_ref()).await {
			Ok(resp) => resp,
			Err(e) => {
				// eprintln!("{} {}: {:?}", method, uri, e);
				http::ResponseBuilder::new().status(http::status_code::INTERNAL_SERVER_ERROR).build().unwrap()
			}
		}
	}
} 


// TODO: Support graceful stopping of HTTP2 servers.
pub async fn run_http_server<
	Arg: 'static + Send + Sync,
	Func: 'static + Send + Sync + for<'a> AsyncFn2<http::Request, &'a Arg, Output=Result<http::Response>>
>(
	port: u16,
	func: Func,
	arg: Arc<Arg>
) {

	let server = http::Server::new(port, RequestHandlerWrap {
		func, arg
	});

	let (tx, rx) = common::async_std::channel::bounded(1);

	let tx1 = tx.clone();
	let handle = common::async_std::task::spawn(async move {
		println!("Listening on http://localhost:{}", port);
		server.run().await;
		tx1.send(()).await;
	});

	let tx2 = tx.clone();
	ctrlc::set_handler(move || {
		// Shutdown the server
		tx2.try_send(());
    }).expect("Error setting Ctrl-C handler");

	rx.recv().await;

	println!("Shutdown!")
}
