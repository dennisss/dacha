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
use raft::protos::*;
use raft::state_machine::*;
use raft::log::*;
use raft::consensus::ConsensusModule;
use clap::{Arg, App, SubCommand};
use std::sync::{Arc, Mutex};
use futures::future::*;


/*
	Some form of client interface is needed so that we can forward arbitrary entries to any server

*/

/*
let mut config = Configuration {
	last_applied: 0, // TODO: Convert to an Option
	members: HashSet::new(),
	learners: HashSet::new()
};

config.members.insert(ServerDescriptor {
	id: 1,
	addr: "http://127.0.0.1:4001".to_string()
});

config.members.insert(ServerDescriptor {
	id: 2,
	addr: "http://127.0.0.1:4002".to_string()
});
*/

fn main() {

	let matches = App::new("Raft")
		.about("Sample consensus reaching node")
		.arg(Arg::with_name("bootstrap")
			.long("bootstrap")
			.help("Indicates that this should be created as the first node in the cluster"))
		/*
		.arg(Arg::with_name("id")
			.long("id")
			.value_name("SERVER_ID")
			.help("Server id for this node (currently should be either 1 or 2)")
			.required(true)
			.takes_value(true))
		*/
		.get_matches();


	//let id = matches.value_of("id").unwrap().parse::<u64>().unwrap(); // Of type ServerId

	let bootstrap = matches.is_present("bootstrap");

	// For now, we will assume that bootstrapping is well known up front

	// Every single server starts with totally empty versions of everything
	let config = Configuration::default();
	let mut meta = Metadata::default();
	let mut log = MemoryLogStore::new();

	let mut id: ServerId;
	let mut is_empty: bool;

	// For the first server in the cluster (assuming no configs are already on disk)
	if bootstrap {

		id = 1;
		is_empty = false;

		log.append(&vec![LogEntry {
			term: 1,
			index: 1,
			data: LogEntryData::Config(ConfigChange::AddMember(ServerDescriptor {
				id: 1,
				addr: "http://127.0.0.1:4001".to_string()
			}))
		}]);

		meta.current_term = 1;
		meta.voted_for = None;
		meta.commit_index = 1;

		// ^ all of this must happen before creating the consensus module in order to not confuse it

	}
	else {

		id = 2;
		is_empty = true;

		// TODO: Get a 

		// Must wait ot get some role in an existing cluster (basically we propose AddLearner on an existing cluster and hopefully, it will start just magically replicating stuff to us)

		// XXX: Although we should only o this after the consensus module is up and running 

	}



	let (inst, event, event_commit) = ConsensusModule::new(id, meta, config, Arc::new(Mutex::new(log)));

	let inst_handle = Arc::new(Mutex::new(inst));

	// In the case of bootstrapping, we must simply force a single entry to be considered commited which contains a config for the first node

	println!("Starting with id {}", id);

	// TODO: Support passing in a port (and maybe also an addr)
	let server = ConsensusModule::start(inst_handle.clone(), event);


	// TODO: If one node joins another cluster with one node, does the old leader of that cluster need to step down?

	let join_cluster = lazy(move || {

		if !is_empty {
			return err(())
		}

		ok(())
	})
	.and_then(|_| {

		// The descriptor for the leader of an existing cluster to join
		let leader = ServerDescriptor {
			id: 1,
			addr: "http://127.0.0.1:4001".into()
		};

		let this = ServerDescriptor {
			id: 2,
			addr: "http://127.0.0.1:4002".into()
		};

		raft::rpc::call_propose(&leader, &raft::protos::ProposeRequest {
			data: LogEntryData::Config(ConfigChange::AddMember(this))
		}).then(|res| -> FutureResult<(), ()> {

			println!("call_propose response: {:?}", res);
			
			ok(())
		})
		
	})
	.then(|_| {
		ok(())
	});

	tokio::run(
		server.join(join_cluster)
		.map(|_| ())	
	);

	// This is where we would perform anything needed to manage regular client requests (and utilize the server handle to perform operations)
	// Noteably we want to respond to clients with nice responses telling them specifically if we are not the actual leader and can't actually fulfill their requests
}

