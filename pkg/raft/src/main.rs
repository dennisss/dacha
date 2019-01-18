#![feature(proc_macro_hygiene, decl_macro, type_alias_enum_variants, generators)]

#[macro_use] extern crate serde_derive;
#[macro_use] extern crate error_chain;

extern crate futures_await as futures;

extern crate rand;
extern crate serde;
extern crate rmp_serde as rmps;
extern crate hyper;
extern crate tokio;
extern crate clap;

extern crate raft;

use raft::errors::*;
use raft::consensus::ConsensusModule;
use clap::{Arg, App, SubCommand};
use std::sync::{Arc, Mutex};


/*
	Some form of client interface is needed so that we can forward arbitrary entries to any server

*/

fn main() {

	let matches = App::new("Raft")
		.about("Sample consensus reaching node")
		.arg(Arg::with_name("id")
			.long("id")
			.value_name("SERVER_ID")
			.help("Server id for this node (currently should be either 1 or 2)")
			.required(true)
			.takes_value(true))
		.get_matches();


	let id = matches.value_of("id").unwrap().parse::<u64>().unwrap(); // Of type ServerId

	let (inst, event) = ConsensusModule::new(id);

	let inst_handle = Arc::new(Mutex::new(inst));

	// In the case of bootstrapping, we must simply force a single entry to be considered commited which contains a config for the first node

	println!("Starting with id {}", id);

	// TODO: Support passing in a port (and maybe also an addr)
	let f = ConsensusModule::start(inst_handle.clone(), event);

	tokio::run(f);

	// This is where we would perform anything needed to manage regular client requests (and utilize the server handle to perform operations)
	// Noteably we want to respond to clients with nice responses telling them specifically if we are not the actual leader and can't actually fulfill their requests
}

