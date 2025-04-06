use std::collections::HashSet;
use std::future::Future;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use common::errors::*;
use common::futures::future::FutureExt;
use common::futures::{pin_mut, select};
use crypto::random;
use crypto::random::RngExt;
use executor::channel;
use executor::sync::Eventually;
use executor_multitask::{
    impl_resource_passthrough, RootResource, ServiceResource, ServiceResourceGroup,
    ServiceResourceSubscriber,
};
use file::dir_lock::DirLock;
use protobuf::{Message, StaticMessage};
use raft_client::server::channel_factory::{self, ChannelFactory};
use raft_client::{
    DiscoveryClient, DiscoveryClientOptions, DiscoveryMulticast, DiscoveryServer,
    RouteChannelFactory, RouteHostnameResolver, RouteStore,
};
use rpc_util::AddReflection;

use crate::atomic::*;
use crate::log::segmented_log::{SegmentedLog, SegmentedLogOptions};
use crate::proto::*;
use crate::server::server::*;
use crate::server::state_machine::*;
use crate::Log;

/*
TODO:
Proper shutdown ordering:
- Ctrl-C
- Node sees the shutdown cue
    - If we are the leader, enter into a 'leader transfer state'
    - Wait for the current server to no longer be the leader
    - Mark ourselves as unelegable for becoming a candidate for a while
    - Mark our RouteStore route as 'not ready' and wait for some propagation period.
- Stop the RPC server
    - Wait for all requests to be satisfied
    - Flush
- Now all the dependencies can be stopped.

But, if the RPC server is dead, a follower may try to start an election
But, if the follower stops operating, the RPC server will stale AppendEntry requests.

So, step 1 is for the Node to notice that we are cancelled and notify the consensus module to stop competing for leadership
Step 2: Wait for the current node to no longer be the leader of the raft group
    TODO: Don't block if it is impossible to form a new leadership majority with only other healthy members.
Step 2: RPC Server lame ducks and shuts down
Step 3: Everything in the server shuts down

So the proper dependency tree is:
- Root
    - raft::Node (with a special cleanup task)
        - rpc::Server
            - raft::Server

During startup:
- raft::Node must wait for

- Extra raft states:
    - Passive
        -

Note that until the main application server is ready to receive requests, the current


*/
/*
Basically issue a timed ban ('Drain') on being the leader:

- Mark consensus module as draining
    - Any new Execute or Propose requests will start to be rejected with a 'Draining' alert
        - User should block and retry once done draining
    - If leader, send a TimeoutNow to the most healthy follower after all
    - There is no stop to heartbeats until we are de-throned.

    DrainState
        - Draining
        - Drained (until time)
        - Undrainable (because there are not enough other followers )


    while drained, it will never start an election

*/

// TODO: Rename back to SimpleServer or StandaloneServer.

/// Configuration for creating a SimpleServer instance.
///
/// TODO: Support disabling listening to multi-cast messages.
pub struct NodeOptions<'a, R> {
    /// Directory in which all server data is stored.
    /// TODO: We should clone a reference to the lock internally so that we can
    /// ensure that the directory isn't re-locked until we stop running all the
    /// threads.
    pub dir: DirLock,

    /// If true, we will create a new empty log with this as the first member of
    /// the corresponding raft group.
    ///
    /// Else (default), we will wait for some other group to show up with a
    /// compatible label set and join it.
    pub bootstrap_group: bool,

    /// If provided and the node hasn't being initialized yet, uses this id as
    /// the id of the current server/node.
    ///
    /// Else (default), generates a unique id internally.
    pub bootstrap_node_id: Option<ServerId>,

    /// TODO: Need to feed these through the hostname resolver?
    pub seed_list: Vec<String>,

    pub hostname_resolver: Arc<dyn RouteHostnameResolver>,

    /// State machine instance to be used.
    pub state_machine: Arc<dyn StateMachine<R> + Send + Sync + 'static>,

    pub log_options: SegmentedLogOptions,

    pub route_labels: Vec<RouteLabel>,

    /// NOTE: A single RPC server can only have one Node instance attached to
    /// it.
    pub rpc_handler: &'a mut rpc::Http2RequestHandler,

    /// Value which becomes available once the RPC server is up and has started accepting RPCs.
    ///
    /// The node will use this to prevent participation in the cluster until we are able
    /// to receive messages from other nodes.
    pub rpc_server_ready: Arc<Eventually<()>>,

    /// Address (ip + port) at which the 'rpc_handler' will be available once it
    /// has been started.
    ///
    /// TODO: Consider instead making an accessor method on the Http2Server to
    /// retrieve this.S
    pub rpc_server_address: String,

    /// TLS options to use for connecting to other nodes.
    pub tls_options: Option<crypto::tls::ClientOptionsContainer>,

    /// If true, setup the standard multicase server and seed rpc client for finding
    /// other nodes.
    pub enable_discovery: bool,
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

    resources: ServiceResourceGroup,

    empty_log: bool,
}

// Same as impl_resource_passthrough (but that macro doesn't support templates).
#[async_trait]
impl<R: 'static + Send> ServiceResource for Node<R> {
    async fn add_cancellation_token(
        &self,
        token: ::std::sync::Arc<dyn executor::cancellation::CancellationToken>,
    ) {
        self.resources.add_cancellation_token(token).await
    }

    async fn new_resource_subscriber(&self) -> Box<dyn ServiceResourceSubscriber> {
        self.resources.new_resource_subscriber().await
    }
}

impl<R: 'static + Send> Node<R> {
    // Creates a new Node instance, attachs it to the server, and starts up
    // background tasks.
    pub async fn create(options: NodeOptions<'_, R>) -> Result<Node<R>> {
        let route_store = RouteStore::new(&options.route_labels, options.hostname_resolver);

        let resources = ServiceResourceGroup::new("raft::Node");

        // Start network discovery.
        // - This is required for bootstrapping.
        // - Note that the route_store hasn't been initialized with our route yet, so it
        //   is ok for this to be started before our RPC server is ready.
        if options.enable_discovery {
            let discovery_multicast = DiscoveryMulticast::create(route_store.clone()).await?;
            resources
                .register_dependency(Arc::new(discovery_multicast.start()))
                .await;

            let discovery_client = DiscoveryClient::create(
                route_store.clone(),
                DiscoveryClientOptions {
                    seeds: options.seed_list,
                    active_broadcaster: true,
                    tls_options: options.tls_options.clone(),
                },
            )
            .await;
            resources
                .spawn_interruptable("raft::DiscoveryClient", discovery_client.run())
                .await;
        }

        let meta_builder =
            BlobFile::builder(&options.dir.path().join("METADATA".to_string())).await?;
        let log_path = options.dir.path().join("log".to_string());

        let log = SegmentedLog::open(log_path, options.log_options).await?;

        // ^ A known issue is that a bootstrapped node will currently not be
        // able to recover if it hasn't fully flushed its own log through the
        // server process

        let channel_factory;

        let (meta, meta_file): (ServerMetadata, BlobFile) = if meta_builder.exists().await? {
            // TODO: Must check that the meta exists and is valid.

            let (meta_file, meta_data) = meta_builder.open().await?;

            let meta = ServerMetadata::parse(&meta_data)?;

            channel_factory = Arc::new(RouteChannelFactory::new(
                route_store.clone(),
                options.tls_options.clone(),
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

            channel_factory = Arc::new(RouteChannelFactory::new(
                route_store.clone(),
                options.tls_options.clone(),
            ));

            let id = {
                if let Some(id) = options.bootstrap_node_id {
                    if options.bootstrap_group {
                        crate::server::bootstrap::bootstrap_first_server_with_id(id, &log).await?;
                    }

                    id
                } else if options.bootstrap_group {
                    crate::server::bootstrap::bootstrap_first_server(&log).await?
                } else {
                    crate::server::bootstrap::generate_new_server_id(
                        channel_factory.as_ref(),
                    )
                    .await?
                }
            };

            println!("Starting new server with id: {}", id.value());

            let mut server_meta = ServerMetadata::default();
            server_meta.set_id(id);

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

        // Start the RPC server.
        // NOTE: The server should be started before we start participating in the raft
        // group.
        options
            .rpc_handler
            .add_service(DiscoveryServer::new(route_store.clone()).into_service())?;
        options
            .rpc_handler
            .add_service(server.clone().into_service())?;

        let rpc_server_ready = options.rpc_server_ready;

        let dir = Arc::new(options.dir);

        {
            let server = server.clone();
            let route_store = route_store.clone();
            let address = options.rpc_server_address.clone();
            let dir_lock = dir.clone();
            let rpc_server_ready = rpc_server_ready.clone();

            resources
                .spawn_interruptable("raft::Server::run", async move {
                    // Wait for the server to be listening.
                    rpc_server_ready.get().await;

                    // Setup the discovery server with our server identity.
                    {
                        let mut route_store = route_store.lock().await;

                        let mut local_route = Route::default();
                        local_route.set_server_id(server.identity().server_id);
                        local_route.target_mut().set_addr(address);

                        route_store.set_local_route(local_route);
                    }

                    // TODO: Verify that in an empty log state, this can't locally become leader.

                    server.run().await?;

                    Ok(())
                })
                .await;
        }

        // TODO: Wait for one round of RPC seeding to elapse. THis will ensure that if
        // we join a cluster, it will know about us.

        // If our log is empty, then we are most likely not a member of the
        // cluster yet
        // So we must attempt to either add ourselves to the cluster or wait
        // until the leader has populated our log with at least one entry
        if empty_log {
            let server = server.clone();
            let route_store = route_store.clone();
            let rpc_server_ready = rpc_server_ready.clone();
            let channel_factory = channel_factory.clone();

            // TODO: If we restart after already receigin this, then we may not need to join
            // again.
            resources
                .spawn_interruptable("raft::Server::join_group", async move {
                    // Wait for the rpc server to be listening.
                    rpc_server_ready.get().await;

                    // As soon as we join the group, the leader will attempt to send us requests so
                    // make sure that most nodes know who we are so that those requests don't fail.
                    crate::check_well_known::check_if_well_known(
                        route_store,
                        channel_factory,
                    )
                    .await?;

                    server.join_group().await
                })
                .await;
        }

        Ok(Self {
            dir,
            route_store,
            channel_factory,
            server,
            resources,
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
}
