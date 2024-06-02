use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::sync::atomic::AtomicUsize;
use std::sync::Arc;

use common::bytes::Bytes;
use common::errors::*;
use datastore_meta_client::constants::*;
use executor::channel;
use executor::child_task::ChildTask;
use executor_multitask::{RootResource, ServiceResource, ServiceResourceGroup};
use file::dir_lock::DirLock;
use file::LocalPathBuf;
use protobuf::Message;
use raft::atomic::{BlobFile, BlobFileBuilder};
use raft::log::segmented_log::SegmentedLogOptions;
use raft::proto::RouteLabel;
use raft::PendingExecutionResult;
use raft::StateMachine;
use rpc_util::{AddProfilingEndpoints, AddReflection};
use sstable::db::{SnapshotIteratorOptions, WriteBatch};
use sstable::iterable::Iterable;

use crate::meta::state_machine::*;
use crate::meta::table_key::TableKey;
use crate::meta::transaction::*;
use crate::proto::*;

/*

Need an event listener on the Server to tell when we become a leader vs. stop being the leader
- If we are not the leader, we need to cancel all transactions.

Limits on transactions:
- max lifetime: 10 seconds

- We are either the leader or we have a
ableKey::user_value(
Should I re-use the internal replication port?
- Pros: Can directly re-use the normal raft server discovery mechanism
- Cons: Difficult to run
*/

// RouteChannel is challenging as it only uses the regular RPC port and not the
// service's one? Must start RPC port before registering currentl port.

/*
Also, the channel factory doesn't do channel caching.
*/

// XXX: If I store the method name in the

pub struct MetastoreOptions {
    /// Path to the directory used to store all of the store's data (at least
    /// this machine's copy).
    pub dir: LocalPathBuf,

    // TODO: Validate that exactly one of init_port and bootstrap are provided.
    /// Port used for listening for RPC signals for bootstrapping this server in
    /// a new cluster.
    ///
    /// Not used for already setup clusters.
    pub init_port: Option<u16>,

    pub bootstrap: bool,

    /// Server port of the RPC service exposed to users of the store.
    /// This will also be used for internal communication between servers.
    pub service_port: u16,

    pub route_labels: Vec<RouteLabel>,

    pub state_machine: EmbeddedDBStateMachineOptions,

    pub log: SegmentedLogOptions,
}

#[derive(Clone)]
pub struct Metastore {
    shared: Arc<Shared>,
}

struct Shared {
    node: Arc<raft::Node<()>>,

    /// NOTE: Only reads go through this object. Writes must go through the
    /// replication_Server.
    state_machine: Arc<EmbeddedDBStateMachine>,

    transaction_manager: TransactionManager,

    next_local_id: AtomicUsize,
}

impl Metastore {
    fn get_client_id<T: protobuf::StaticMessage>(request: &rpc::ServerRequest<T>) -> Result<&str> {
        match request.context.metadata.get_text(CLIENT_ID_KEY) {
            Ok(Some(v)) => Ok(v),
            _ => Err(rpc::Status::invalid_argument(
                "Invalid or missing client id in request context",
            )
            .into()),
        }
    }

    /// CANCEL SAFE
    async fn snapshot_impl<'a>(
        &self,
        request: rpc::ServerRequest<SnapshotRequest>,
        response: &mut rpc::ServerResponse<'a, SnapshotResponse>,
    ) -> Result<()> {
        if !request.latest() {
            return Err(rpc::Status::invalid_argument("Unsupported snapshotting method").into());
        }

        // TODO: If we know it's going to be used for a transaction, we should use the
        // optimistic mode.
        let read_index = self
            .shared
            .node
            .server()
            .begin_read(request.optimistic())
            .await
            .map_err(|e| e.to_rpc_status())?;

        let snapshot = self.shared.state_machine.snapshot().await;

        // NOTE: This may be < the read_index as raft config changes aren't applied to
        // the state machine.
        response.set_read_index(snapshot.last_sequence());

        Ok(())
    }

    /// CANCEL SAFE
    async fn read_impl<'a>(
        &self,
        request: rpc::ServerRequest<ReadRequest>,
        response: &mut rpc::ServerStreamResponse<'a, ReadResponse>,
    ) -> Result<()> {
        self.shared
            .node
            .server()
            .begin_read(request.read_index() != 0)
            .await
            .map_err(|e| e.to_rpc_status())?;

        // TODO: Support changing to a specific read_index (will require checking the
        // flush index).
        let snapshot = self.shared.state_machine.snapshot().await;

        let start_key = TableKey::user_value(request.keys().start_key());
        let end_key = TableKey::user_value(request.keys().end_key());

        let mut iter_options = SnapshotIteratorOptions::default();
        if request.read_index() > 0 {
            if request.read_index() < snapshot.compaction_waterline().unwrap() {
                return Err(rpc::Status::aborted("Request's read_index is too old.").into());
            }

            iter_options.last_sequence = Some(request.read_index());
        }

        let mut iter = snapshot.iter_with_options(iter_options).await?;
        iter.seek(&start_key).await?;

        while let Some(entry) = iter.next().await? {
            // TODO: Use a proper key comparator.
            if &entry.key[..] >= &end_key[..] {
                break;
            }

            let table_key = TableKey::parse(&entry.key)?;

            let user_key = match table_key {
                TableKey::UserData {
                    user_key,
                    sub_key: UserDataSubKey::USER_VALUE,
                } => user_key,
                _ => continue,
            };

            let user_value = match entry.value {
                Some(value) => value,
                None => {
                    // Deleted
                    continue;
                }
            };

            let mut res = ReadResponse::default();
            res.entry_mut().set_key(&user_key[..]);
            res.entry_mut().set_value(user_value.as_ref());
            res.entry_mut().set_sequence(entry.sequence);

            response.send(res).await?;
        }

        Ok(())
    }

    async fn execute_impl<'a>(
        &self,
        request: rpc::ServerRequest<ExecuteRequest>,
        response: &mut rpc::ServerResponse<'a, ExecuteResponse>,
    ) -> Result<()> {
        // Translate to an internally keyed transaction
        let user_txn = request.value.transaction();
        let mut internal_txn = Transaction::default();

        internal_txn.set_read_index(user_txn.read_index());

        for range in user_txn.reads() {
            let mut internal_range = KeyRange::default();
            internal_range.set_start_key(TableKey::user_value(range.start_key()));
            internal_range.set_end_key(TableKey::user_value(range.end_key()));
            internal_txn.add_reads(internal_range);
        }

        for op in user_txn.writes() {
            let mut internal_op = op.as_ref().clone();
            internal_op.set_key(TableKey::user_value(op.key()));
            internal_txn.add_writes(internal_op);
        }

        let index = self
            .shared
            .transaction_manager
            .execute(
                internal_txn,
                self.shared.node.clone(),
                self.shared.state_machine.clone(),
            )
            .await?;

        response.value.set_read_index(index.value());

        Ok(())
    }

    // TODO: This can be implemented on any follower server if we pull changes from
    // the state machine.
    //
    // TODO: Support ignoring changes from the same client as the one that initiated
    // the watch?
    async fn watch_impl<'a>(
        &self,
        request: rpc::ServerRequest<WatchRequest>,
        response: &mut rpc::ServerStreamResponse<'a, WatchResponse>,
    ) -> Result<()> {
        let client_id = Self::get_client_id(&request)?;

        let registration = self
            .shared
            .state_machine
            .watchers()
            .register(request.key_prefix())
            .await;

        // Send head so that the client can properly syncronize the time at which
        // watching starts.
        response.send_head().await?;

        // TODO: Must translate back to user keys.
        // XXX: ^ Yes.

        // TODO: If we ever stop being the leader (or we believe that we are a follower
        // that is significantly out of sync, then we should perform a cancellation from
        // the server after removing ourselves from the serving set).

        loop {
            let res = registration.recv().await?;
            response.send(res).await?;
        }
    }

    async fn new_unique_id(&self) -> Result<String> {
        // If this succeeds, then we know that we were the leader in the given term.
        // If we have a locally unique value, we can make it globally unique by
        // prepending this term.
        let term = self
            .shared
            .node
            .server()
            .begin_read(true)
            .await
            .map_err(|e| e.to_rpc_status())?
            .term();

        let index = self
            .shared
            .next_local_id
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);

        Ok(format!("{}:{}", term.value(), index))
    }

    async fn config_change(&self, request: &ConfigChangeRequest) -> Result<()> {
        let mut entry = raft::proto::LogEntryData::default();
        match request.change_case() {
            ConfigChangeRequestChangeCase::RemoveServer(id) => {
                entry.config_mut().set_RemoveServer(id.clone());
            }
            ConfigChangeRequestChangeCase::NOT_SET => {
                return Err(rpc::Status::invalid_argument("Invalid config change").into());
            }
        }

        let pending_execution = self
            .shared
            .node
            .server()
            .execute(entry)
            .await
            .map_err(|e| Error::from(e.to_rpc_status()))?;

        let commited_index = match pending_execution.wait().await {
            PendingExecutionResult::Committed { log_index, .. } => log_index,
            PendingExecutionResult::Cancelled => {
                return Err(err_msg("Cancelled"));
            }
        };

        Ok(())
    }
}

#[async_trait]
impl ClientManagementService for Metastore {
    async fn NewClient(
        &self,
        request: rpc::ServerRequest<protobuf_builtins::google::protobuf::Empty>,
        response: &mut rpc::ServerResponse<NewClientResponse>,
    ) -> Result<()> {
        response.value.set_client_id(self.new_unique_id().await?);
        Ok(())
    }
}

#[async_trait]
impl ServerManagementService for Metastore {
    async fn ConfigChange(
        &self,
        request: rpc::ServerRequest<ConfigChangeRequest>,
        resposne: &mut rpc::ServerResponse<protobuf_builtins::google::protobuf::Empty>,
    ) -> Result<()> {
        self.config_change(&request.value).await
    }

    async fn CurrentStatus(
        &self,
        req: rpc::ServerRequest<protobuf_builtins::google::protobuf::Empty>,
        res: &mut rpc::ServerResponse<raft::proto::Status>,
    ) -> Result<()> {
        res.value = self.shared.node.server().current_status().await?;
        Ok(())
    }

    async fn Drain(
        &self,
        req: rpc::ServerRequest<DrainRequest>,
        res: &mut rpc::ServerResponse<protobuf_builtins::google::protobuf::Empty>,
    ) -> Result<()> {
        if req.server_id() != self.shared.node.server().identity().server_id {
            return Err(rpc::Status::invalid_argument("Drain request sent to wrong server").into());
        }

        self.shared.node.server().drain().await?;

        Ok(())
    }
}

#[async_trait]
impl KeyValueStoreService for Metastore {
    async fn Snapshot(
        &self,
        request: rpc::ServerRequest<SnapshotRequest>,
        response: &mut rpc::ServerResponse<SnapshotResponse>,
    ) -> Result<()> {
        self.snapshot_impl(request, response).await
    }

    async fn Read(
        &self,
        request: rpc::ServerRequest<ReadRequest>,
        response: &mut rpc::ServerStreamResponse<ReadResponse>,
    ) -> Result<()> {
        self.read_impl(request, response).await
    }

    async fn Execute(
        &self,
        request: rpc::ServerRequest<ExecuteRequest>,
        response: &mut rpc::ServerResponse<ExecuteResponse>,
    ) -> Result<()> {
        self.execute_impl(request, response).await
    }

    async fn Watch(
        &self,
        request: rpc::ServerRequest<WatchRequest>,
        response: &mut rpc::ServerStreamResponse<WatchResponse>,
    ) -> Result<()> {
        self.watch_impl(request, response).await
    }
}

pub async fn run(options: MetastoreOptions) -> Result<Arc<dyn ServiceResource>> {
    if !file::exists(&options.dir).await? {
        file::create_dir(&options.dir).await?;
    }

    let dir = DirLock::open(&options.dir).await?;

    let service = Arc::new(ServiceResourceGroup::new("Metastore"));

    // TODO: Add a resource dependency on this. Should be stopped after the RPC
    // server
    let state_machine =
        Arc::new(EmbeddedDBStateMachine::open(&options.dir, &options.state_machine).await?);
    service.register_dependency(state_machine.clone()).await;

    // TODO: Must limit what percentage of request slots can be used for user facing
    // requests since we also use this for server-to-server Raft requests.
    let mut rpc_server = rpc::Http2Server::new(Some(options.service_port));

    let local_address = http::uri::Authority {
        user: None,
        host: http::uri::Host::IP(net::local_ip()?),
        port: Some(options.service_port),
    }
    .to_string()?;

    // TODO: Add the state machine as a dependency of the node.
    let node = Arc::new(
        raft::Node::create(raft::NodeOptions {
            dir,
            init_port: options.init_port,
            bootstrap: options.bootstrap,
            seed_list: vec![], // Will just find everyone via multi-cast
            state_machine: state_machine.clone(),
            log_options: options.log,
            route_labels: options.route_labels.clone(),
            rpc_server: &mut rpc_server,
            rpc_server_address: local_address,
        })
        .await?,
    );

    service.register_dependency(node.clone()).await;

    let instance = Metastore {
        shared: Arc::new(Shared {
            node: node.clone(),
            state_machine,
            transaction_manager: TransactionManager::new(),
            next_local_id: AtomicUsize::new(1),
        }),
    };

    rpc_server.add_service(Arc::new(raft::LeaderServiceWrapper::new(
        node.clone(),
        ClientManagementIntoService::into_service(instance.clone()),
    )))?;

    rpc_server.add_service(Arc::new(raft::LeaderServiceWrapper::new(
        node.clone(),
        KeyValueStoreIntoService::into_service(instance.clone()),
    )))?;

    rpc_server.add_service(Arc::new(raft::LeaderServiceWrapper::new(
        node.clone(),
        ServerManagementIntoService::into_service(instance.clone()),
    )))?;

    rpc_server.add_reflection()?;
    rpc_server.add_profilez()?;

    service.register_dependency(rpc_server.start()).await;

    Ok(service)
}
