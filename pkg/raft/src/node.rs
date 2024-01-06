use std::future::Future;
use std::sync::Arc;

use common::errors::*;
use common::futures::future::FutureExt;
use common::futures::{pin_mut, select};
use crypto::random;
use crypto::random::RngExt;
use executor::bundle::TaskResultBundle;
use executor::channel;
use executor::sync::Mutex;
use file::dir_lock::DirLock;
use protobuf::{Message, StaticMessage};
use raft_client::{
    DiscoveryClient, DiscoveryMulticast, DiscoveryServer, RouteChannelFactory, RouteStore,
};
use rpc_util::AddReflection;

use crate::atomic::*;
use crate::log::segmented_log::{SegmentedLog, SegmentedLogOptions};
use crate::proto::*;
use crate::server::server::*;
use crate::server::state_machine::*;
use crate::Log;

/// Configuration for creating a SimpleServer instance.
///
/// TODO: Support disabling listening to multi-cast messages.
pub struct NodeOptions<R> {
    /// Directory in which all server data is stored.
    /// TODO: We should clone a reference to the lock internally so that we can
    /// ensure that the directory isn't re-locked until we stop running all the
    /// threads.
    pub dir: DirLock,

    /// Port used to use the init service if this is an un-initialized server.
    pub init_port: u16,

    pub bootstrap: bool,

    pub seed_list: Vec<String>,

    /// State machine instance to be used.
    pub state_machine: Arc<dyn StateMachine<R> + Send + Sync + 'static>,

    pub route_labels: Vec<RouteLabel>,
    /* pub task_bundle: &'a mut TaskResultBundle, */

    /* /// NOTE: A single RPC server can only have one Node instance attached to
     * /// it.
     * pub rpc_server: &'a mut rpc::Http2Server, */

    /* /// Address (ip + port) at which the 'rpc_server' will be available once it
     * /// has been started.
     * ///
     * /// TODO: Consider instead making an accessor method on the Http2Server to
     * /// retrieve this.S
     * pub rpc_server_address: String, */
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
    dir: Arc<DirLock>,

    route_store: RouteStore,

    channel_factory: Arc<RouteChannelFactory>,

    server: Server<R>,

    task_bundle: Option<TaskResultBundle>,

    empty_log: bool,
}

impl<R: 'static + Send> Node<R> {
    // Creates a new Node instance, attachs it to the server, and starts up
    // background tasks.
    pub async fn create(options: NodeOptions<R>) -> Result<Node<R>> {
        let route_store = RouteStore::new(&options.route_labels);

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

        let meta_builder =
            BlobFile::builder(&options.dir.path().join("METADATA".to_string())).await?;
        let log_path = options.dir.path().join("log".to_string());

        let log = SegmentedLog::open(log_path, SegmentedLogOptions::default()).await?;

        // ^ A known issue is that a bootstrapped node will currently not be
        // able to recover if it hasn't fully flushed its own log through the
        // server process

        let channel_factory;

        let (meta, meta_file): (ServerMetadata, BlobFile) = if meta_builder.exists().await? {
            // TODO: Must check that the meta exists and is valid.

            let (meta_file, meta_data) = meta_builder.open().await?;

            let meta = ServerMetadata::parse(&meta_data)?;

            channel_factory = Arc::new(RouteChannelFactory::new(
                meta.group_id(),
                route_store.clone(),
            ));

            (meta, meta_file)
        }
        // Otherwise we are starting a new server instance
        else {
            if options.state_machine.last_applied().await > 0.into()
                || log.last_index().await > 0.into()
            {
                return Err(err_msg(
                    "Missing raft state, but have non-empty data. Possible corruption?",
                ));
            }

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
                    let found_peer = raft_client::utils::find_peer_group_id(&route_store).fuse();
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
                            // Most likely the program is currently being shut down.
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

            // We save the meta file to disk last such that if the meta file exists, then we
            // know that we have a complete set of files on disk
            let meta_file = meta_builder.create(&server_meta.serialize()?).await?;

            (server_meta, meta_file)
        };

        println!("Starting with id {}", meta.id().value());

        let initial_state = ServerInitialState {
            meta,
            meta_file,
            log: Box::new(log),
            state_machine: options.state_machine,
        };

        let empty_log = initial_state.log.last_index().await.value() == 0;

        println!(
            "Initial commit index: {}",
            initial_state.meta.meta().commit_index().value()
        );

        let server = Server::new(channel_factory.clone(), initial_state).await?;

        Ok(Self {
            dir: Arc::new(options.dir),
            route_store,
            channel_factory,
            server,
            task_bundle: Some(task_bundle),
            empty_log,
        })
    }

    pub fn id(&self) -> ServerId {
        self.server.identity().server_id
    }

    pub fn server(&self) -> &Server<R> {
        &self.server
    }

    pub fn channel_factory(&self) -> &RouteChannelFactory {
        &self.channel_factory
    }

    pub fn run(
        &mut self,
        rpc_server: &mut rpc::Http2Server,
        address: &str,
    ) -> Result<impl Future<Output = Result<()>> + 'static> {
        let mut task_bundle = self
            .task_bundle
            .take()
            .ok_or_else(|| err_msg("Node can only be run once"))?;

        // Start the RPC server.
        // NOTE: The server should be started before we start participating in the raft
        // group.
        rpc_server.add_service(DiscoveryServer::new(self.route_store.clone()).into_service())?;
        rpc_server.add_service(self.server.clone().into_service())?;

        let (started_sender, started_receiver) = channel::bounded(2);
        rpc_server.add_start_callback(move || {
            // Wkae up both threads.
            for i in 0..2 {
                let _ = started_sender.try_send(());
            }
        });

        let route_store = self.route_store.clone();
        let server = self.server.clone();

        let started_receiver2 = started_receiver.clone();

        let address = address.to_string();
        let dir_lock = self.dir.clone();
        task_bundle.add("raft::Server", async move {
            // Wait for the server to be listening.
            started_receiver.recv().await?;

            // Setup the discovery server with our server identity.
            {
                let mut route_store = route_store.lock().await;

                let mut local_route = Route::default();
                local_route.set_group_id(server.identity().group_id);
                local_route.set_server_id(server.identity().server_id);
                local_route.set_addr(address);

                route_store.set_local_route(local_route);
            }

            server.run().await?;

            // TODO: Instead drop it once all threads are joined.
            drop(dir_lock);
            Ok(())
        });

        // TODO: Wait for one round of RPC seeding to elapse. THis will ensure that if
        // we join a cluster, it will know about us.

        // If our log is empty, then we are most likely not a member of the
        // cluster yet
        // So we must attempt to either add ourselves to the cluster or wait
        // until the leader has populated our log with at least one entry
        if self.empty_log {
            let server = self.server.clone();

            task_bundle.add("raft::Server::join_group", async move {
                started_receiver2.recv().await?;
                server.join_group().await
            });
        }

        Ok(async move { task_bundle.join().await })
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
        let _ = self.sender.try_send(());
        Ok(())
    }
}

impl ServerInit {
    async fn wait_for_init(port: u16) -> Result<()> {
        let (sender, receiver) = channel::bounded(1);

        let mut rpc_server = ::rpc::Http2Server::new();
        rpc_server.add_service(Self { sender }.into_service())?;
        rpc_server.add_reflection()?;

        executor::future::race(rpc_server.run(port), async move {
            receiver.recv().await?;
            Ok(())
        })
        .await
    }
}
