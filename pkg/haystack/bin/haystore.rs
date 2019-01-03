#![feature(proc_macro_hygiene, decl_macro, type_alias_enum_variants)]

extern crate haystack;
extern crate clap;
extern crate hyper;

use haystack::directory::Directory;
use haystack::store::machine::StoreMachine;
use haystack::errors::*;
use std::sync::{Arc,Mutex};
use hyper::{Body, Method, Request, Response, Server, StatusCode};
use hyper::service::service_fn;
use clap::{Arg, App};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use hyper::rt::Future;


fn main() -> Result<()> {

	let matches = App::new("Haystore")
		.about("The storage layer")
		.arg(Arg::with_name("port")
			.short("p")
			.long("port")
			.value_name("PORT")
			.help("Sets the listening http port")
			.takes_value(true))
		.arg(Arg::with_name("store")
			.short("f")
			.long("folder")
			.value_name("FOLDER")
			.help("Sets the data directory for store volumes")
			.takes_value(true))
		.get_matches();

	let port = matches.value_of("port").unwrap_or("4000").parse::<u16>().expect("Invalid port given");
	let store = matches.value_of("store").unwrap_or("/hay");

	let dir = Directory::open()?;

	let machine = StoreMachine::load(dir, port, store)?;
	let mac_handle = Arc::new(Mutex::new(machine));

	// TODO: See https://docs.rs/hyper/0.12.19/hyper/server/struct.Server.html#example for graceful shutdowns

	let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), port);

	let mac_server = mac_handle.clone();

	let server = Server::bind(&addr)
        .serve(move || {
			let mac_client = mac_server.clone();
			service_fn(move |req: Request<Body>| {
				haystack::store::routes::handle_request(
					mac_client.clone(), req
				)
			})
		})
        .map_err(|e| eprintln!("server error: {}", e));

    println!("Listening on http://{}", addr);

	StoreMachine::start(mac_handle);

    hyper::rt::run(server);

	Ok(())
}
