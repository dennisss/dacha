use std::sync::Arc;
use std::time::Duration;

use common::async_std::sync::Mutex;
use common::async_std::task;
use common::errors::*;
use common::fs::DirLock;
use crypto::random;
use crypto::random::RngExt;
use protobuf::Message;

use crate::atomic::*;
use crate::log::log::*;
use crate::log::log_metadata::LogSequence;
use crate::log::simple_log::*;
use crate::proto::consensus::*;
use crate::proto::routing::*;
use crate::proto::server_metadata::*;
use crate::routing::discovery_client::DiscoveryClient;
use crate::routing::discovery_server::DiscoveryServer;
use crate::routing::route_channel::*;
use crate::routing::route_store::RouteStore;
use crate::server::channel_factory::*;
use crate::server::server::*;
use crate::server::state_machine::*;

/*
    Safety considerations:
    - If we have a non-empty state machine, then we must have a metadata file

    - Ideally this will also be what manages the routes file
        - Importantly the routes file can only be stored on disk if we also have
          a metadata file present
            - Otherwise it is invalid
*/

/*
    Other concerns:
    - Making sure that the routes data always stays well saved on disk
    - Won't change frequently though

    - We will be making our own identity here though
*/

/*
TODO: If we are going to use a read index for performing a blind write operation, then we don't need need to do any waiting to acquire a read index.

The basic discovery service:
- Broadcast to all servers without any cluster id
- Later we will filter routes by group id.
- Most of it should fit within

*/

pub struct NodeConfig<R> {
    pub dir: DirLock,
    pub bootstrap: bool,
    pub seed_list: Vec<String>,
    pub state_machine: Arc<dyn StateMachine<R> + Send + Sync + 'static>,
    pub last_applied: LogIndex,
}

/// Meant to be one layer removed from the Server interface
pub struct Node<R> {
    /// Duplicated id for convenience
    /// TODO: Could probably be better specified in terms of the server instance
    pub id: ServerId,

    pub dir: DirLock,

    // TODO: Is this used for anything?
    pub server: Server<R>,

    // TODO: Decouple all the discovery stuff from the Node
    routes_file: Mutex<BlobFile>,
}

impl<R: 'static + Send> Node<R> {
    // Ideally will produce a promise that generates a running node and then
    pub async fn start(config: NodeConfig<R>) -> Result<Arc<Node<R>>> {
        // TODO: Verify that we never start up with snapshots that begin before
        // the beginning of our log

        // Ideally an agent would encapsulate saving itself to disk via some file
        // somewhere
        let route_store = Arc::new(Mutex::new(RouteStore::new()));

        let discovery_client = DiscoveryClient::new(route_store.clone(), config.seed_list);

        // Basically need to get a:
        // (meta, meta_file, config_snapshot, config_file, log_file)

        let meta_builder = BlobFile::builder(&config.dir.path().join("meta".to_string())).await?;
        let config_builder =
            BlobFile::builder(&config.dir.path().join("config".to_string())).await?;
        let routes_builder =
            BlobFile::builder(&config.dir.path().join("routes".to_string())).await?;
        let log_path = config.dir.path().join("log".to_string());

        // If a previous instance was started in this directory, restart it
        // NOTE: In this case we will ignore the bootstrap flag
        // TODO: Need good handling of missing files that doesn't involve just
        // deleting everything
        // ^ A known issue is that a bootstrapped node will currently not be
        // able to recover if it hasn't fully flushed its own log through the
        // server process

        let channel_factory;

        let (meta, meta_file, config_snapshot, config_file, log, routes_file): (
            ServerMetadata,
            BlobFile,
            ServerConfigurationSnapshot,
            BlobFile,
            SimpleLog,
            BlobFile,
        ) = if meta_builder.exists().await {
            let (meta_file, meta_data) = meta_builder.open().await?;

            // TODO: In most cases, we can survive without having a routes file
            // on disk or even a config file in many cases
            let (config_file, config_data) = config_builder.open().await?;
            let (routes_file, routes_data) = routes_builder.open().await?;
            let mut log = SimpleLog::open(&log_path).await?;

            let meta = ServerMetadata::parse(&meta_data)?;
            let config_snapshot = ServerConfigurationSnapshot::parse(&config_data)?;

            // Restore any saved route information.
            {
                let ann = Announcement::parse(&routes_data)?;
                let mut route_store = route_store.lock().await;
                route_store.apply(&ann);
            }

            // a.group_id = Some(meta.group_id()); // < Otherwise this also gets configured
            // in Server::start, but we require that
            // // it be set in order to apply a routes list

            channel_factory = Arc::new(RouteChannelFactory::new(
                meta.group_id(),
                route_store.clone(),
            ));

            (
                meta,
                meta_file,
                config_snapshot,
                config_file,
                log,
                routes_file,
            )
        }
        // Otherwise we are starting a new server instance
        else {
            // In general, we should never be creating state machine snapshots
            // before persisting our core raft state as we use the group_id to
            // ensure that the correct log is being used for the state machine
            // Therefore if this does happen, then somehow the raft specific
            // files were deleted leaving only the state machine
            if config.last_applied > 0.into() {
                panic!(
                    "Can not trust already state machine data without \
						corresponding metadata"
                )
            }

            // Cleanup any old partially written files
            // TODO: Log when this occurs
            config_builder.purge().await?;
            routes_builder.purge().await?;
            SimpleLog::purge(&log_path).await?;

            // Every single server starts with totally empty versions of everything
            let meta = crate::proto::consensus_state::Metadata::default();
            let config_snapshot = ServerConfigurationSnapshot::default();
            let mut log = vec![];

            let id: ServerId;
            let group_id: GroupId;

            // For the first server in the cluster (assuming no configs are
            // already on disk)
            if config.bootstrap {
                id = 1.into();

                // Assign a cluster id to our agent (usually would be retrieved
                // through network discovery if not in bootstrap mode)
                group_id = random::clocked_rng().uniform::<u64>().into();

                channel_factory = Arc::new(RouteChannelFactory::new(group_id, route_store.clone()));

                // For this to be supported, we must be able to become a leader with zero
                // members in the config (implying that we can know if we are )
                let mut first_entry = LogEntry::default();
                first_entry.pos_mut().set_term(1);
                first_entry.pos_mut().set_index(1);
                first_entry.data_mut().config_mut().set_AddMember(id);

                log.push(first_entry);
            } else {
                // TODO: All of this could be in while loop until we are able to
                // connect to the leader and propose a new message on it

                discovery_client.seed().await?;

                // Pick an arbitrary group from the set of discovered groups.
                group_id = *route_store
                    .lock()
                    .await
                    .remote_groups()
                    .iter()
                    .next()
                    .unwrap();

                channel_factory = Arc::new(RouteChannelFactory::new(group_id, route_store.clone()));

                // TODO: Must start the discovery background thread before running this.

                id = crate::server::bootstrap::generate_new_server_id(
                    group_id,
                    channel_factory.as_ref(),
                )
                .await?;

                println!("Generated new server id: {}", id.value());
            }

            let mut server_meta = ServerMetadata::default();
            server_meta.set_id(id);
            server_meta.set_group_id(group_id);
            server_meta.set_meta(meta);

            let log_file = SimpleLog::create(&log_path).await?;

            let mut seq = LogSequence::zero();

            // TODO: Can we do this before creating the log so that everything
            // is flushed to disk What we could do is say that if the metadata
            // file is present, then
            for e in log {
                let next_seq = seq.next();
                seq = next_seq;

                log_file.append(e, next_seq).await?;
            }

            log_file.flush().await?;

            let config_file = config_builder.create(&config_snapshot.serialize()?).await?;

            let routes_file = routes_builder
                .create(&route_store.lock().await.serialize().serialize()?)
                .await?;

            // We save the meta file to disk last such that if the meta file exists, then we
            // know that we have a complete set of files on disk
            let meta_file = meta_builder.create(&server_meta.serialize()?).await?;

            (
                server_meta,
                meta_file,
                config_snapshot,
                config_file,
                log_file,
                routes_file,
            )
        };

        println!("Starting with id {}", meta.id().value());
        let our_id = meta.id();

        let port = 4000 + (meta.id().value() as u16);
        println!("PORT: {}", port);

        // Setup the discovery server with our server identity.
        {
            let mut route_store = route_store.lock().await;

            let mut local_route = Route::default();
            local_route.set_group_id(meta.group_id());
            local_route.set_server_id(meta.id());
            // TODO: this is subject to change if we are running over HTTPS
            local_route.set_addr(format!("http://127.0.0.1:{}", port));

            route_store.set_local_route(local_route);
        }

        let initial_state = ServerInitialState {
            meta,
            meta_file,
            config_snapshot,
            config_file,
            log: Box::new(log),
            state_machine: config.state_machine,
            last_applied: config.last_applied,
        };

        let is_empty = initial_state.log.last_index().await.value() == 0;

        println!(
            "COMMIT INDEX {}",
            initial_state.meta.meta().commit_index().value()
        );

        let server = Server::new(channel_factory, initial_state).await;

        // TODO: We shouldn't announce our local route until the server is running.

        // Start the RPC server.
        {
            // TODO: We also need to add a DiscoveryService (DiscoveryServiceRouter)
            let mut rpc_server = ::rpc::Http2Server::new();
            rpc_server.add_service(DiscoveryServer::new(route_store.clone()).into_service())?;
            rpc_server.add_service(server.clone().into_service())?;

            // TODO: Finally if possible we should attempt to broadcast our ip
            // address to other servers so they can rediscover us

            task::spawn(async move {
                if let Err(e) = rpc_server.run(port).await {
                    eprintln!("{:?}", e);
                }
            });
        }

        // TODO: Support passing in a port (and maybe also an addr)
        task::spawn(server.clone().run());

        // TODO: this must start running earlier to support dynamic re-seeding during
        // startup proposals.
        task::spawn(discovery_client.run());

        // TODO: If one node joins another cluster with one node, does the old leader of
        // that cluster need to step down?

        // THe simpler way to think of this is (if not bootstrap mode and there are zero
        // ) But yeah, if we can get rid of the bootstrap caveat, then this i

        // TODO: Will also need to spawn the task that will periodically save
        // the routes when changed

        // If our log is empty, then we are most likely not a member of the
        // cluster yet
        // So we must attempt to either add ourselves to the cluster or wait
        // until the leader has populated our log with at least one entry
        if is_empty {
            println!("Planning on joining: ");

            // TODO: This may fail if we previously attemped to join the cluster, but we
            // failed to get the response. Now it could be possible that we can't propose
            // anything to the cluster as no one can become the leader. So, we should

            // TODO: Possibly build another layer of client that will do the
            // extra discovery and leader_hint caching

            // For anything to work properly, this must occur after we have an
            // id,

            // XXX: at this point, we should know who the leader is with better
            // precision than this  (based on a leader hint from above)

            // TODO: Should always first become a learner.

            server.join_group().await?;
        }

        let node = Arc::new(Node {
            id: our_id,
            dir: config.dir,
            server,
            routes_file: Mutex::new(routes_file),
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
