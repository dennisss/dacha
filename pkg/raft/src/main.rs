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
extern crate core;

use raft::errors::*;
use raft::protos::*;
use raft::state_machine::*;
use raft::log::*;
use raft::consensus::ConsensusModule;
use raft::server::Server;
use raft::atomic::*;
use raft::rpc::{marshal, unmarshal};
use raft::server_protos::*;
use std::path::Path;
use clap::{Arg, App, SubCommand};
use std::sync::{Arc, Mutex};
use futures::future::*;
use core::DirLock;


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

// XXX: See https://github.com/etcd-io/etcd/blob/fa92397e182286125c72bf52d95f9f496f733bdf/raft/raft.go#L113 for more useful config parameters

/*
	Bootstrap first node:
	- server_id:
		- 1
	- log:
		- 1 log entry (containing ConfigChange)
			- term 1, index 1
	- config
		- 1 member in config (containing self)
		- Naturally any config that is not fully up-to-date can be made up-to-date 
	- meta
		- current_term: 1
		- voted_for: None
		- commit_index: 1
	- state_machine
		- empty

	Adding new server to cluster
*/



/*
	In order to make a server, we must at least have a server id 
	- First and for-most, if there already exists a file on disk with metadata, then we should use that
	- Otherwise, we must just block until we have a machine id by some other method
		- If an existing cluster exists, then we will ask it to make a new cluster id
		- Otherwise, the main() script must wait for someone to bootstrap us and give ourselves id 1
*/


/*
	Other scenarios
	- Server startup
		- Server always starts completely idle and in a mode that would reject external requests
		- If we have configuration on disk already, then we can use that
		- If we start with a join cli flag, then we can:
			- Ask the cluster to create a new unique machine id (we could trivially use an empty log entry and commit that to create a new id) <- Must make sure this does not conflict with the master's id if we make many servers before writing other data
	
		- If we are sent a one-time init packet via http post, then we will start a new cluster on ourselves

*/

/*
	Summary of event variables:
	- OnCommited
		- Ideally this would be a channel tht can pass the Arc references to the listeners so that maybe we don't need to relock in order to take things out of the log
		- ^ This will be consumed by clients waiting on proposals to be written and by the state machine thread waiting for the state machine to get fully applied 
	- OnApplied
		- Waiting for when a change is applied to the state machine
	- OnWritten
		- Waiting for when a set of log entries have been persisted to the log file
	- OnStateChange
		- Mainly to wake up the cycling thread so that it can 
		- ^ This will always only have a single consumer so this may always be held as light weight as possibl


	TODO: Future optimization would be to also save the metadata into the log file so that we are only ever writing to one append-only file all the time
		- I think this is how etcd implements it as well
*/


fn main() -> Result<()> {

	let matches = App::new("Raft")
		.about("Sample consensus reaching node")
		.arg(Arg::with_name("dir")
			.long("dir")
			.short("d")
			.value_name("DIRECTORY_PATH")
			.help("An existing directory to store data file for this unique instance")
			.required(true)
			.takes_value(true))
		// TODO: Also support specifying our rpc listening port
		.arg(Arg::with_name("join")
			.long("join")
			.short("j")
			.value_name("SERVER_ADDRESS")
			.help("Address of a running server to be used for joining its cluster if this instance has not been initialized yet")
			.takes_value(true))
		.arg(Arg::with_name("bootstrap")
			.long("bootstrap")
			.help("Indicates that this should be created as the first node in the cluster"))
		.get_matches();


	// TODO: For now, we will assume that bootstrapping is well known up front although eventually to enforce that it only ever occurs exactly once, we may want to have an admin externally fire exactly one request to trigger it
	// But even if we do pass in bootstrap as an argument, it is still guranteed to bootstrap only once on this machine as we will persistent the bootstrapped configuration before talking to other servers in the cluster

	let dir = Path::new(matches.value_of("dir").unwrap());
	let bootstrap = matches.is_present("bootstrap");

	let lock = DirLock::open(&dir)?;


	// Basically need to get a (meta, meta_file, config_snapshot, config_file, log_file)

	let meta_builder = BlobFile::builder(&dir.join("meta".to_string()))?;
	let config_builder = BlobFile::builder(&dir.join("config".to_string()))?;

	let mut is_empty: bool;

	// If a previous instance was started in this directory, restart it
	// NOTE: In this case we will ignore the bootstrap flag
	// TODO: Need good handling of missing files that doesn't involve just deleting everything
	// TODO: Must also 
	let (
		meta, meta_file,
		config_snapshot, config_file,
		log
	) : (
		ServerMetadata, BlobFile,
		ServerConfigurationSnapshot, BlobFile,
		MemoryLogStorage
	) = if meta_builder.exists() || config_builder.exists() {

		let (meta_file, meta_data) = meta_builder.open()?;
		let (config_file, config_data) = config_builder.open()?;

		// TODO: Load from disk
		let mut log = MemoryLogStorage::new();

		let meta = unmarshal(meta_data)?;
		let config_snapshot = unmarshal(config_data)?;

		is_empty = false;

		(meta, meta_file, config_snapshot, config_file, log)
	}
	// Otherwise we are starting a new server instance
	else {
		// Every single server starts with totally empty versions of everything
		let mut meta = Metadata::default();
		let config_snapshot = ServerConfigurationSnapshot::default();
		let mut log = MemoryLogStorage::new();


		let mut id: ServerId;

		// For the first server in the cluster (assuming no configs are already on disk)
		if bootstrap {

			id = 1;
			is_empty = false;

			log.append(LogEntry {
				term: 1,
				index: 1,
				data: LogEntryData::Config(ConfigChange::AddMember(1))
				/* ServerDescriptor {
					id: 1,
					addr: "http://127.0.0.1:4001".to_string()
				})) */
			});

			// TODO: Also make it durable (otherwise we would be violating the fact that a majority of servers have a match_index >= the the commit_index)

		}
		else {

			// Ask the cluster for a config
			// Then ask the server for its set of routes 

			// In summary one new-member request which 

			id = 2;
			is_empty = true;

			// TODO: In this case we should probably be bootstrapping our routes from another server in the cluster already

			// TODO: Get a 

			// Must wait ot get some role in an existing cluster (basically we propose AddLearner on an existing cluster and hopefully, it will start just magically replicating stuff to us)

			// XXX: Although we should only o this after the consensus module is up and running 

		}

		let server_meta = ServerMetadata {
			id, cluster_id: 0,
			meta
		};

		let meta_file = meta_builder.create(&marshal(&server_meta)?)?;
		let config_file = config_builder.create(&marshal(&config_snapshot)?)?;

		// TODO: The config should get immediately comitted and we should immediately safe it with the right cluster id (otherwise this bootstrap will just result in us being left with a totally empty config right?)
		// ^ Although it doesn't really matter all that much

		(
			server_meta, meta_file,
			config_snapshot, config_file,
			log
		)
	};

	println!("Starting with id {}", meta.id);

	let log = Arc::new(log);

	// TODO: It would be better to have this be created by the server so that we can properly passthrough everything
	let inst = ConsensusModule::new(meta.id, meta.meta, config_snapshot.config, log.clone());

	// TODO: This also needs to be given the saved routes from the configuration
	let server = Server::new(inst, log.clone(), meta_file, config_file);

	let server_handle = Arc::new(server);

	// In the case of bootstrapping, we must simply force a single entry to be considered commited which contains a config for the first node


	// TODO: Support passing in a port (and maybe also an addr)
	let task = Server::start(server_handle.clone());
	
	let mut state_machine = MemoryKVStateMachine::new();

	let apply_commits = || {
		
		// Apply to the changes
		// If someone is 
		

	};



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

		raft::rpc::call_propose(&leader.addr, &raft::protos::ProposeRequest {
			data: LogEntryData::Config(ConfigChange::AddMember(this.id))
		}).then(|res| -> FutureResult<(), ()> {

			println!("call_propose response: {:?}", res);
			
			ok(())
		})
		
	})
	.then(|_| {
		ok(())
	});

	tokio::run(
		task.join(join_cluster)
		.map(|_| ())	
	);

	// This is where we would perform anything needed to manage regular client requests (and utilize the server handle to perform operations)
	// Noteably we want to respond to clients with nice responses telling them specifically if we are not the actual leader and can't actually fulfill their requests

	Ok(())
}

