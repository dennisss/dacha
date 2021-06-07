use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::{Arc, Mutex};


// use hyper::{Request, Response, Body, Server, StatusCode};
// use hyper::http::request::Parts;
// use hyper::service::service_fn;

use common::errors::*;

// use super::errors::Error;
// use futures::TryFutureExt;

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

pub fn json_response<T>(code: http::status_code::StatusCode, obj: &T) -> http::Response where T: serde::Serialize {
	let body = serde_json::to_string(obj).unwrap();
	Response::builder()
		.status(code)
		.header("Content-Type", "application/json; charset=utf-8")
		.body(Body::from(body))
		.unwrap()
}

pub fn text_response(code: http::status_code::StatusCode, text: &'static str) -> http::Response {
	Response::builder()
		.status(code)
		.header("Content-Type", "text/plain; charset=utf-8")
		.body(Body::from(text))
		.unwrap()
}

/// Wraps a regular async request in a wrapper that logs out errors and nicely responds to clients on errors
/// NOTE: The error type doesn't really matter as we never resolve to a error, just as long as it is sendable across threads, hyper won't complain
pub fn handle_request_guard<F, P, I>(
	req: Request<Body>, arg: I, f: F,
) -> impl Future<Output=std::result::Result<Response<Body>, std::io::Error>>
	where P: Future<Output=std::result::Result<Response<Body>, Error>>,
		  I: Clone,
		  F: Fn(Parts, Body, I) -> P {

	let (parts, body) = req.into_parts();

	// Mainly for being able to print out errors
	let method = parts.method.clone();
	let uri = parts.uri.clone();

	f(parts, body, arg).then(move |res| {
		match res {
			Ok(resp) => Ok(resp),
			Err(e) => {
				eprintln!("{} {}: {:?}", method, uri, e);
				Ok(Response::builder().status(500).body(Body::empty()).unwrap())
			}
		}
	})
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

// TODO: See https://docs.rs/hyper/0.12.19/hyper/server/struct.Server.html#example for graceful shutdowns
pub fn start_http_server<F, FS, FE, P: 'static, I: 'static>(
	port: u16, arg: &Arc<I>, f: &'static F, fstart: &FS, fend: &'static FE
)
	where P: Send + Future<Output=std::result::Result<Response<Body>, Error>>,
		  I: Send + Sync,
		  F: Sync + (Fn(Parts, Body, Arc<I>) -> P),
		  FS: Fn(&Arc<I>),
		  FE: Sync + Fn(&Arc<I>)
{
	let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), port);

	let (tx, rx) = futures::channel::oneshot::channel::<()>();

	let arg = arg.clone();
	let arg2 = arg.clone();
	let arg3 = arg.clone();
	let server = Server::bind(&addr)
        .serve(move || {
			let arg = arg.clone();
			service_fn(move |req: Request<Body>| {
				handle_request_guard(req, arg.clone(), f).compat()			
			})
		})
		.with_graceful_shutdown(rx)
		.compat()
		.map_err(|e| eprintln!("HTTP Server Error: {}", e));

    println!("Listening on http://{}", addr);
	

	let tx_wrap = Arc::new(Mutex::new(Some(tx)));
	ctrlc::set_handler(move || {

		// Take the tx exactly once (all future ctrl-c's will get a None and return)
		let tx = match tx_wrap.lock().unwrap().take() {
			Some(tx) => tx,
			None => return
		};

		// Everything below here should only ever be called exactly once

		fend(&arg2);

		// Shutdown the server
		if let Err(e) = tx.send(()) {
			eprintln!("Error while shutting down: {:?}", e);
		}

    }).expect("Error setting Ctrl-C handler");

	fstart(&arg3);

	hyper::rt::run(server);

	println!("Shutdown!")
}

