#![feature(proc_macro_hygiene, decl_macro, type_alias_enum_variants, generators)]

#[macro_use] extern crate serde_derive;
#[macro_use] extern crate error_chain;

extern crate futures_await as futures;

extern crate rand;
extern crate serde;
extern crate rmp_serde as rmps;
extern crate hyper;
extern crate tokio;

extern crate raft;

use raft::errors::*;
use raft::server::Server;

use std::sync::{Arc, Mutex};


fn main() {

	let s = Arc::new(Mutex::new(
		Server::new()
	));

	// TODO: Support passing in a port (and maybe also an addr)
	Server::start(s.clone());

	// This is where we would perform anything needed to manage regular client requests (and utilize the server handle to perform operations)
	// Noteably we want to respond to clients with nice responses telling them specifically if we are not the actual leader and can't actually fulfill their requests
	loop {
		std::thread::sleep_ms(1000);
	}
}

