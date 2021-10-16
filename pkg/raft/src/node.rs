use std::sync::Arc;

use common::async_std::channel;
use common::async_std::sync::Mutex;
use common::bundle::TaskResultBundle;
use common::errors::*;
use common::fs::DirLock;
use common::futures::future::FutureExt;
use common::futures::{pin_mut, select};
use crypto::random;
use crypto::random::RngExt;
use protobuf::Message;

use crate::atomic::*;
use crate::log::segmented_log::SegmentedLog;
use crate::proto::consensus::*;
use crate::proto::init::*;
use crate::proto::routing::*;
use crate::proto::server_metadata::*;
use crate::routing::discovery_client::DiscoveryClient;
use crate::routing::discovery_server::DiscoveryServer;
use crate::routing::route_channel::*;
use crate::routing::route_store::{RouteStore, RouteStoreHandle};
use crate::server::server::*;
use crate::server::state_machine::*;
use crate::DiscoveryMulticast;
use crate::Log;

/// Configuration for creating a SimpleServer instance.
///
/// TODO: Support disabling listening to multi-cast messages.
pub struct NodeOptions<R> {
    /// Directory in which all server data is stored.
    pub dir: DirLock,

    /// Port used to use the init service if this is an un-initialized server.
    /// Requests to start a bootstrap  
    pub init_port: u16,

    pub bootstrap: bool,

    pub seed_list: Vec<String>,

    /// State machine instance to be used
    pub state_machine: Arc<dyn StateMachine<R> + Send + Sync + 'static>,

    /// Index of the last log entry applied to the state machine. For a newly
    /// created state machine, this would be zero.
    pub last_applied: LogIndex,
}

/// Simple implementation of a complete Raft based server instance.
///
/// If all you need is to run a single Raft group based server, then using this
/// wrapper is probably simplest way to do so as it will take of common setup
/// details.
///
/// Usage:
/// - Use create() to construct a new SimpleServer (from new or existing data).
/// - Create your own frontend service using a raft::Server instance retrieving
///   from Node::server().
/// - Then call run() in a background thread to run the Raft consensus RPC
///   service and replication code.
///   - NOTE: Until this thread is running, you won't be able to execute any
///     commands on the server successfully.
///
/// Compared to directly using raft::Server, this also manages:
/// - Network discovery of other servers
/// - Server initialization bootstrapping or joining.
/// - RPC channel implementation
///
/// The server runs out of a single directory with the following files:
/// - ./STATE: Contains the serialized Raft persistent state.
/// - ./CONFIG: Contains the latest snapshot of the Raft configuration state
///   machine which contains the list of all raft group members.
/// - ./log/:  Directory used to store segmented log files.
///
/// The user is recommended to add the following files as well:
/// - ./LOCK:  File for acquiring an exclusive lock to read/write from this
///   directory.
/// - ./CURRENT : File containing a pointer to the latest snapshot of the state
///   machine.
/// - ./snapshot-00000N : Directory containing the N'th snapshot of the state
///   machine.
///   - When the state machines decides that it is ready to flush to disk, it
///     may create a new snapshot directory, update the CURRENT file and delete
///     the old snapshot.
///   - Alternatively if the the state machine supports atomically advancing an
///     old snapshot forward in time, it may modify the current snapshot
///     in-place.
///   - To implement restore()'ing from a remote snapshot, the state machine can
///     implement this similarly by creating a new snapshot directory to house
///     the new snapshot while it is being flushed to disk and then later switch
///     over to it by updating the CURRENT file once the full snapshot has been
///     received and flushed to disk.
pub struct Node<R> {
    /// Stored in the struct in order to maintain the lock while the server is
    /// alive.
    dir: DirLock,

    route_store: RouteStoreHandle,

    server: Server<R>,

    task_bundle: TaskResultBundle,

    empty_log: bool,
}

impl<R: 'static + Send> Node<R> {
    // Ideally will produce a promise that generates a running node and then
    pub async fn create(options: NodeOptions<R>) -> Result<Node<R>> {
        let route_store = Arc::new(Mutex::new(RouteStore::new()));

        let mut task_bundle = TaskResultBundle::new();

        // Start network discovery.
        // NOTE: Must happen before server initialization as new server initialization
        // requires being able to discover existing servers.
        // - Also needed for initial bootstrappign
        {
            let discovery_multicast = DiscoveryMulticast::create(route_store.clone()).await?;
            task_bundle.add("DiscoveryMulticast", async move {
                discovery_multicast.run().await
            });

            let discovery_client = DiscoveryClient::new(route_store.clone(), options.seed_list);
            task_bundle.add("DiscoveryClient", async {
                discovery_client.run().await;
                Ok(())
            });
        }

        let meta_builder = BlobFile::builder(&options.dir.path().join("STATE".to_string())).await?;
        let config_builder =
            BlobFile::builder(&options.dir.path().join("CONFIG".to_string())).await?;
        let log_path = options.dir.path().join("log".to_string());

        let log = SegmentedLog::open(log_path, 32 * 1024 * 1024).await?;

        // ^ A known issue is that a bootstrapped node will currently not be
        // able to recover if it hasn't fully flushed its own log through the
        // server process

        let channel_factory;

        let (meta, meta_file, config_snapshot, config_file): (
            ServerMetadata,
            BlobFile,
            ServerConfigurationSnapshot,
            BlobFile,
        ) = if meta_builder.exists().await {
            // TODO: Must check that the meta exists and is valid.

            let (meta_file, meta_data) = meta_builder.open().await?;

            let (config_file, config_data) = config_builder.open().await?;

            let meta = ServerMetadata::parse(&meta_data)?;
            let config_snapshot = ServerConfigurationSnapshot::parse(&config_data)?;

            channel_factory = Arc::new(RouteChannelFactory::new(
                meta.group_id(),
                route_store.clone(),
            ));

            (meta, meta_file, config_snapshot, config_file)
        }
        // Otherwise we are starting a new server instance
        else {
            if options.last_applied > 0.into() || log.last_index().await > 0.into() {
                return Err(err_msg(
                    "Missing raft state, but have non-empty data. Possible corruption?",
                ));
            }

            // Cleanup any old partially written files
            // TODO: Log when this occurs
            config_builder.purge().await?;

            // Every single server starts with totally empty versions of everything
            let meta = crate::proto::consensus_state::Metadata::default();
            let config_snapshot = ServerConfigurationSnapshot::default();

            // Get a group id (or None to imply that we should bootstrap a new group).
            //
            // When not bootstraping we will wait for either:
            // - A background error to occur
            // - A ServerInit::Bootstrap RPC to come to tell us to bootstrap ourselves
            // - We discover a peer on the network that is already initialized.
            let group_id_or_bootstrap: Option<GroupId> = {
                if options.bootstrap {
                    None
                } else {
                    let init_signal = ServerInit::wait_for_init(options.init_port).fuse();
                    let found_peer = Self::find_peer_group_id(route_store.clone()).fuse();
                    let background_error = task_bundle.join().fuse();

                    pin_mut!(init_signal, found_peer, background_error);

                    select! {
                        res = init_signal => {
                            res?;
                            None
                        }
                        gid = found_peer => {
                            Some(gid)
                        }
                        res = background_error => {
                            res?;
                            return Err(err_msg("Discovery thread exited early"));
                        }
                    }
                }
            };

            let (group_id, bootstrap) = match group_id_or_bootstrap {
                Some(gid) => (gid, false),
                None => (random::clocked_rng().uniform::<u64>().into(), true),
            };

            channel_factory = Arc::new(RouteChannelFactory::new(group_id, route_store.clone()));

            let id = if bootstrap {
                crate::server::bootstrap::bootstrap_first_server(&log).await?
            } else {
                crate::server::bootstrap::generate_new_server_id(group_id, channel_factory.as_ref())
                    .await?
            };

            println!("Starting new server with id: {}", id.value());

            let mut server_meta = ServerMetadata::default();
            server_meta.set_id(id);
            server_meta.set_group_id(group_id);
            server_meta.set_meta(meta);

            let config_file = config_builder.create(&config_snapshot.serialize()?).await?;

            // We save the meta file to disk last such that if the meta file exists, then we
            // know that we have a complete set of files on disk
            let meta_file = meta_builder.create(&server_meta.serialize()?).await?;

            (server_meta, meta_file, config_snapshot, config_file)
        };

        println!("Starting with id {}", meta.id().value());

        let initial_state = ServerInitialState {
            meta,
            meta_file,
            config_snapshot,
            config_file,
            log: Box::new(log),
            state_machine: options.state_machine,
            last_applied: options.last_applied,
        };

        let empty_log = initial_state.log.last_index().await.value() == 0;

        println!(
            "Initial commit index: {}",
            initial_state.meta.meta().commit_index().value()
        );

        let server = Server::new(channel_factory, initial_state).await?;

        Ok(Self {
            dir: options.dir,
            route_store,
            server,
            task_bundle,
            empty_log,
        })
    }

    async fn find_peer_group_id(route_store: RouteStoreHandle) -> GroupId {
        loop {
            let remote_groups = {
                let route_store = route_store.lock().await;
                route_store.remote_groups()
            };

            if remote_groups.is_empty() {
                common::async_std::task::sleep(std::time::Duration::from_secs(1)).await;
                continue;
            }

            return *remote_groups.iter().next().unwrap();
        }
    }

    pub fn id(&self) -> ServerId {
        self.server.identity().server_id
    }

    pub fn server(&self) -> Server<R> {
        self.server.clone()
    }

    pub fn route_store(&self) -> RouteStoreHandle {
        self.route_store.clone()
    }

    pub async fn run(mut self, port: u16) -> Result<()> {
        // let port = 4000 + (meta.id().value() as u16);
        // println!("PORT: {}", port);

        // Start the RPC server.
        // NOTE: The server should be started before we start participating in the raft
        // group.
        {
            let mut rpc_server = ::rpc::Http2Server::new();
            rpc_server
                .add_service(DiscoveryServer::new(self.route_store.clone()).into_service())?;
            rpc_server.add_service(self.server.clone().into_service())?;

            // TODO: Add cancellation token here.

            // TODO: Ideally block until the socket listener is setup so that future code
            // can depend on our server being reachable.
            self.task_bundle.add("rpc::Server", rpc_server.run(port));
        }

        // Setup the discovery server with our server identity.
        // TODO: Can't do this until the server is running.
        {
            let mut route_store = self.route_store.lock().await;

            let mut local_route = Route::default();
            local_route.set_group_id(self.server.identity().group_id);
            local_route.set_server_id(self.server.identity().server_id);
            // TODO: this is subject to change if we are running over HTTPS
            local_route.set_addr(format!("http://127.0.0.1:{}", port));

            route_store.set_local_route(local_route);
        }

        // TODO: Wait for one round of RPC seeding to elapse. THis will ensure that if
        // we join a cluster, it will know about us.

        self.task_bundle
            .add("raft::Server", self.server.clone().run());

        // THe simpler way to think of this is (if not bootstrap mode and there are zero
        // ) But yeah, if we can get rid of the bootstrap caveat, then this i

        // tODO: Start another task for doing joining.

        // If our log is empty, then we are most likely not a member of the
        // cluster yet
        // So we must attempt to either add ourselves to the cluster or wait
        // until the leader has populated our log with at least one entry
        if self.empty_log {
            let server = self.server();
            self.task_bundle
                .add("raft::Server::join_group", async move {
                    server.join_group().await
                });
        }

        self.task_bundle.join().await
    }
}

/// Simple RPC service implementation that just waits until a user calls it once
/// to let us know that it.
struct ServerInit {
    sender: channel::Sender<()>,
}

#[async_trait]
impl ServerInitService for ServerInit {
    async fn Bootstrap(
        &self,
        req: rpc::ServerRequest<BootstrapRequest>,
        res: &mut rpc::ServerResponse<BootstrapResponse>,
    ) -> Result<()> {
        let _ = self.sender.send(());
        Ok(())
    }
}

impl ServerInit {
    async fn wait_for_init(port: u16) -> Result<()> {
        let (sender, receiver) = channel::bounded(1);

        let mut rpc_server = ::rpc::Http2Server::new();
        rpc_server.add_service(Self { sender }.into_service())?;

        common::future::race(rpc_server.run(port), async move {
            receiver.recv().await?;
            Ok(())
        })
        .await
    }
}
