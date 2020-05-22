use std::sync::Arc;
use std::time::Duration;
use common::async_std::task;
use common::async_std::sync::Mutex;
use common::errors::*;
use common::fs::DirLock;
use rand::prelude::*;
use crate::protos::*;
use crate::rpc::*;
use crate::discovery::*;
use crate::atomic::*;
use crate::routing::*;
use crate::state_machine::*;
use crate::server::*;
use crate::server_protos::*;
use crate::log::*;
use crate::simple_log::*;
use common::async_std::prelude::*;
use common::async_std::future;
use common::futures::FutureExt;

/*
	Safety considerations:
	- If we have a non-empty state machine, then we must have a metadata file

	- Ideally this will also be what manages the routes file
		- Importantly the routes file can only be stored on disk if we also have
		  a metadata file present
			- Otherwise it is invalid
*/

pub struct NodeConfig<R> {
	pub dir: DirLock,
	pub bootstrap: bool,
	pub seed_list: Vec<String>,
	pub state_machine: Arc<dyn StateMachine<R> + Send + Sync + 'static>,
	pub last_applied: LogIndex
}

/// Meant to be one layer removed from the Server interface 
pub struct Node<R> {
	/// Duplicated id for convenience
	/// TODO: Could probably be better specified in terms of the server instance 
	pub id: ServerId,

	pub dir: DirLock,
	pub server: Arc<Server<R>>,
	pub discovery: Arc<DiscoveryService>,

	routes_file: Mutex<BlobFile>
}

impl<R: 'static + Send> Node<R> {
	// Ideally will produce a promise that generates a running node and then
	pub async fn start(config: NodeConfig<R>) -> Result<Arc<Node<R>>> {

		// TODO: Verify that we never start up with snapshots that begin before
		// the beginning of our log

		// Ideally an agent would encapsulate saving itself to disk via some file somewhere
		let agent = Arc::new(Mutex::new( NetworkAgent::new() ));

		let client = Arc::new(Client::new(agent.clone()));
		let discovery = Arc::new(
			DiscoveryService::new(client.clone(), config.seed_list));

		// Basically need to get a:
		// (meta, meta_file, config_snapshot, config_file, log_file)

		let meta_builder = BlobFile::builder(
			&config.dir.path().join("meta".to_string()))?;
		let config_builder = BlobFile::builder(
			&config.dir.path().join("config".to_string()))?;
		let routes_builder = BlobFile::builder(
			&config.dir.path().join("routes".to_string()))?;
		let log_path = config.dir.path().join("log".to_string());


		// If a previous instance was started in this directory, restart it
		// NOTE: In this case we will ignore the bootstrap flag
		// TODO: Need good handling of missing files that doesn't involve just
		// deleting everything
		// ^ A known issue is that a bootstrapped node will currently not be
		// able to recover if it hasn't fully flushed its own log through the
		// server process

		let (
			meta, meta_file,
			config_snapshot, config_file,
			log, routes_file
		) : (
			ServerMetadata, BlobFile,
			ServerConfigurationSnapshot, BlobFile,
			SimpleLog, BlobFile
		) = if meta_builder.exists() {

			let (meta_file, meta_data) = meta_builder.open()?;

			// TODO: In most cases, we can survive without having a routes file
			// on disk or even a config file in many cases
			let (config_file, config_data) = config_builder.open()?;
			let (routes_file, routes_data) = routes_builder.open()?;
			let mut log = SimpleLog::open(&log_path).await?;

			let meta: ServerMetadata = unmarshal(meta_data.as_ref())?;
			let config_snapshot = unmarshal(config_data.as_ref())?;

			let ann: Announcement = unmarshal(routes_data.as_ref())?;
			let mut a = agent.lock().await;
			a.cluster_id = Some(meta.cluster_id); // < Otherwise this also gets configured in Server::start, but we require that it be set in order to apply a routes list
			a.apply(&ann);	
		
			(meta, meta_file, config_snapshot, config_file, log, routes_file)
		}
		// Otherwise we are starting a new server instance
		else {

			// In general, we should never be creating state machine snapshots
			// before persisting our core raft state as we use the cluster_id to
			// ensure that the correct log is being used for the state machine
			// Therefore if this does happen, then somehow the raft specific
			// files were deleted leaving only the state machine
			if config.last_applied > 0 {
				panic!("Can not trust already state machine data without \
						corresponding metadata")
			}

			// Cleanup any old partially written files
			// TODO: Log when this occurs
			config_builder.purge()?;
			routes_builder.purge()?;
			SimpleLog::purge(&log_path)?;


			// Every single server starts with totally empty versions of everything
			let mut meta = super::protos::Metadata::default();
			let config_snapshot = ServerConfigurationSnapshot::default();
			let mut log = vec![];

			let mut id: ServerId;
			let mut cluster_id: ClusterId;

			// For the first server in the cluster (assuming no configs are
			// already on disk)
			if config.bootstrap {

				id = 1;

				// Assign a cluster id to our agent (usually would be retrieved
				// through network discovery if not in bootstrap mode)
				cluster_id = 0; // rand::thread_rng().next_u64();

				log.push(LogEntry {
					pos: LogPosition {
						term: 1,
						index: 1
					},
					data: LogEntryData::Config(ConfigChange::AddMember(1))
				});
			}
			else {
				// TODO: All of this could be in while loop until we are able to
				// connect to the leader and propose a new message on it

				discovery.seed().await?;

				// TODO: Instead pick a random one from our list
				// TODO: This is currently our only usage of .routes() on the
				// agent
				let first_id = agent.lock().await.routes().values().next().unwrap().desc.id;

				let ret = client.call_propose(first_id, &ProposeRequest {
					data: LogEntryData::Noop,
					wait: true
				}).await?;

				// TODO: If we get here, we may get a not_leader, in which case,
				// if we don't have information on the leader's identity, then
				// we need to ask everyone we know for a new list of server
				// addrs

				println!("Generated new index {}", ret.index);

				id = ret.index;

				cluster_id = agent.lock().await.cluster_id.clone()
					.expect("No cluster_id obtained during initial cluster connection");

			}

			let server_meta = ServerMetadata {
				id, cluster_id,
				meta
			};

			let log_file = SimpleLog::create(&log_path).await?;

			// TODO: Can we do this before creating the log so that everything
			// is flushed to disk What we could do is say that if the metadata
			// file is present, then
			for e in log {
				log_file.append(e).await;  // TODO: Convert to a Result<>?
			}

			log_file.flush().await?;

			let config_file = config_builder.create(&marshal(
				&config_snapshot)?)?;
			
			let routes_file = routes_builder.create(&marshal(
				&agent.lock().await.serialize())?)?;

			// We save the meta file to disk last such that if the meta file exists, then we know that we have a complete set of files on disk
			let meta_file = meta_builder.create(&marshal(&server_meta)?)?;

			(
				server_meta, meta_file,
				config_snapshot, config_file,
				log_file, routes_file
			)
		};

		println!("Starting with id {}", meta.id);


		let initial_state = ServerInitialState {
			meta, meta_file,
			config_snapshot, config_file,
			log: Box::new(log),
			state_machine: config.state_machine,
			last_applied: config.last_applied
		};

		let is_empty = initial_state.log.last_index().await == 0;

		println!("COMMIT INDEX {}", initial_state.meta.meta.commit_index);

		let server = Arc::new(Server::new(client.clone(), initial_state).await);

		// TODO: Support passing in a port (and maybe also an addr)
		task::spawn(Server::start(server.clone()).map(|_| ()));
		task::spawn(DiscoveryService::run(discovery.clone()).map(|_| ()));

		// TODO: If one node joins another cluster with one node, does the old leader of that cluster need to step down?

		// THe simpler way to think of this is (if not bootstrap mode and there are zero )
		// But yeah, if we can get rid of the bootstrap caveat, then this i 

		let our_id = client.agent().lock().await.identity.clone().unwrap().id;

		// TODO: Will also need to spawn the task that will periodically save
		// the routes when changed


		// If our log is empty, then we are most likely not a member of the
		// cluster yet
		// So we must attempt to either add ourselves to the cluster or wait
		// until the leader has populated our log with at least one entry
		if is_empty {
			println!("Planning on joining: ");

			// TODO: Possibly build another layer of client that will do the
			// extra discovery and leader_hint caching

			// For anything to work properly, this must occur after we have an
			// id,

			// XXX: at this point, we should know who the leader is with better
			// precision than this  (based on a leader hint from above)

			let res = client.call_propose(1, &ProposeRequest {
				data: LogEntryData::Config(ConfigChange::AddMember(our_id)),
				wait: false
			}).await?;
			println!("call_propose response: {:?}", res);
		}

		let node = Arc::new(Node {
			id: our_id,
			dir: config.dir,
			server,
			discovery,
			routes_file: Mutex::new(routes_file)
		});

		task::spawn(Self::routes_sync(node.clone()));

		Ok(node)
	}

	/// This is a background task which will periodically check if our locally
	/// discovered table of routes has changed and if it has, this will save a
	/// cached copy of them to disk
	/// TODO: In the case of planned shutdowns, we should support having this
	/// immediately flush
	async fn routes_sync(inst: Arc<Self>) {

		loop {
			// TODO: Right here perform the disk syncing

			common::wait_for(Duration::from_millis(5000)).await;
		}
	} 


}

