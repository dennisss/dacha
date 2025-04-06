use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::sync::atomic::AtomicUsize;
use std::sync::Arc;
use std::time::Duration;

use common::bytes::Bytes;
use common::errors::*;
use db_txn_client::constants::*;
use db_txn_client::TransactionalDBClient;
use db_table::key_utils::prefix_key_range;
use executor::channel;
use executor::child_task::ChildTask;
use executor::sync::Eventually;
use executor_multitask::{RootResource, ServiceResource, ServiceResourceGroup};
use file::dir_lock::DirLock;
use file::{LocalPathBuf, LocalPath};
use protobuf::Message;
use raft::atomic::{BlobFile, BlobFileBuilder};
use raft::log::segmented_log::SegmentedLogOptions;
use raft::proto::RouteLabel;
use raft::PendingExecutionResult;
use raft::StateMachine;
use rpc_util::{AddProfilingEndpoints, AddReflection};
use sstable::db::{Snapshot, SnapshotIteratorOptions, WriteBatch};
use sstable::iterable::Iterable;
use db_txn_proto::db::txn::*;

use crate::state_machine::*;
use crate::transaction::*;
use crate::acl_processor::ACLProcessor;

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

pub struct TransactionalDBOptions {
    /// Path to the directory used to store all of the store's data (at least
    /// this machine's copy).
    pub dir: LocalPathBuf,

    pub state_machine: EmbeddedDBStateMachineOptions,

    pub log: SegmentedLogOptions,

    pub bootstrap_group: bool,

    pub bootstrap_node_id: Option<u64>,

    /// Server port of the RPC service exposed to users of the store.
    /// This will also be used for internal communication between servers.
    pub service_port: u16,

    pub route_labels: Vec<RouteLabel>,

    pub hostname_resolver: Arc<dyn raft_client::RouteHostnameResolver>,

    pub tls: Option<crypto::tls::Credentials>,

    pub acl_processor: Option<Arc<dyn ACLProcessor>>,
}

#[derive(Clone)]
pub struct TransactionalDB {
    shared: Arc<Shared>,
}

struct Shared {
    node: Arc<raft::Node<()>>,

    /// NOTE: Only reads go through this object. Writes must go through the
    /// replication_Server.
    state_machine: Arc<EmbeddedDBStateMachine>,

    transaction_manager: TransactionManager,

    next_local_id: AtomicUsize,

    acl_processor: Option<Arc<dyn ACLProcessor>>,
}

impl TransactionalDB {
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

        let snapshot = self.shared.state_machine.snapshot().await;

        let start_key = request.keys().start_key();
        let end_key = request.keys().end_key();

        if end_key <= start_key {
            return Err(
                rpc::Status::invalid_argument("Reading an empty (or negative) key range").into(),
            );
        }

        // NOTE: This is called after 'begin_read' so that we know we are on the leader
        // and are checking ACLs against a recent snapshot.
        if let Some(processor) = &self.shared.acl_processor {
            processor
                .before_read(
                    &snapshot,
                    std::slice::from_ref(request.keys()),
                    &request.context,
                )
                .await?;
        }

        let mut iter_options = SnapshotIteratorOptions::default();
        if request.read_index() > 0 {
            if request.read_index() < snapshot.compaction_waterline() {
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

            let value = match entry.value {
                Some(value) => value,
                None => {
                    // Deleted
                    continue;
                }
            };

            let mut res = ReadResponse::default();
            res.entry_mut().set_key(&entry.key[..]);
            res.entry_mut().set_value(value.as_ref());
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
        // TODO: Move all the request validation in 'execute()' to here.
        // - Also validate that reads are sequential and non-overlapping.

        // ACL checked are applied before entering the transaction logic to avoid
        // acquiring reader/writer locks on any rows.
        if let Some(processor) = &self.shared.acl_processor {
            // Avoid returning a potentially stale ACL response if we are not executing on
            // the lader.
            //
            // TODO: Instead acquire a read index?
            self.shared
                .node
                .server()
                .currently_leader()
                .await
                .map_err(|e| rpc::Status::unavailable("Not currently the leader"))?;

            let snapshot = self.shared.state_machine.snapshot().await;

            processor
                .before_execute(
                    &snapshot,
                    request.value.transaction(),
                    // std::slice::from_ref(request.keys()),
                    &request.context,
                )
                .await?;
        }

        let index = self
            .shared
            .transaction_manager
            .execute(
                request.value.transaction().clone(),
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
    //
    // TODO: Limit max time of one stream so that we eventually re-check ACLs
    async fn watch_impl<'a>(
        &self,
        request: rpc::ServerRequest<WatchRequest>,
        response: &mut rpc::ServerStreamResponse<'a, WatchResponse>,
    ) -> Result<()> {
        // let client_id = Self::get_client_id(&request)?;

        if let Some(processor) = &self.shared.acl_processor {
            // Avoid returning a potentially stale ACL response if we are not executing on
            // the lader.
            //
            // TODO: Instead acquire a read index?
            self.shared
                .node
                .server()
                .currently_leader()
                .await
                .map_err(|e| rpc::Status::unavailable("Not currently the leader"))?;

            let mut key_range = KeyRange::default();
            let (s, e) = prefix_key_range(request.key_prefix());
            key_range.set_start_key(s.as_ref());
            key_range.set_end_key(e.as_ref());

            let snapshot = self.shared.state_machine.snapshot().await;

            processor
                .before_read(
                    &snapshot,
                    std::slice::from_ref(&key_range),
                    &request.context,
                )
                .await?;
        }

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
impl ClientManagementService for TransactionalDB {
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
impl ServerManagementService for TransactionalDB {
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
impl KeyValueStoreService for TransactionalDB {
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

impl TransactionalDB {

    pub async fn create(
        options: TransactionalDBOptions,
        rpc_handler: &mut rpc::Http2RequestHandler,
        rpc_server_ready: Arc<Eventually<()>>
    ) -> Result<Arc<dyn ServiceResource>> {
        if !file::exists(&options.dir).await? {
            file::create_dir(&options.dir).await?;
        }

        let dir = DirLock::open(&options.dir).await?;

        let service = Arc::new(ServiceResourceGroup::new("TransactionalDB"));

        // TODO: Add a resource dependency on this. Should be stopped after the RPC
        // server
        let state_machine =
            Arc::new(EmbeddedDBStateMachine::open(&options.dir, &options.state_machine).await?);
        service.register_dependency(state_machine.clone()).await;

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
                bootstrap_group: options.bootstrap_group,
                bootstrap_node_id: options.bootstrap_node_id.map(|v| v.into()),
                seed_list: vec![], // Will just find everyone via multi-cast
                state_machine: state_machine.clone(),
                log_options: options.log,
                route_labels: options.route_labels.clone(),
                rpc_handler,
                rpc_server_ready,
                rpc_server_address: local_address,
                hostname_resolver: options.hostname_resolver,
                tls_options: options.tls.map(|c| c.client.clone()),
                enable_discovery: options.service_port != 0,
            })
            .await?,
        );

        service.register_dependency(node.clone()).await;

        let instance = TransactionalDB {
            shared: Arc::new(Shared {
                node: node.clone(),
                state_machine,
                transaction_manager: TransactionManager::new(),
                next_local_id: AtomicUsize::new(1),
                acl_processor: options.acl_processor,
            }),
        };

        rpc_handler.add_service(Arc::new(raft::LeaderServiceWrapper::new(
            node.clone(),
            ClientManagementIntoService::into_service(instance.clone()),
        )))?;

        rpc_handler.add_service(Arc::new(raft::LeaderServiceWrapper::new(
            node.clone(),
            KeyValueStoreIntoService::into_service(instance.clone()),
        )))?;

        rpc_handler.add_service(Arc::new(raft::LeaderServiceWrapper::new(
            node.clone(),
            ServerManagementIntoService::into_service(instance.clone()),
        )))?;

        Ok(service)
    }

    /*
    TODO: There are a number of optimizations this could implement since it is a purely in-process DB:
    - Zero copy RPC over LocalChannel
    - Can immediately discard log once snapshot is executed (since don't need to replicate to other servers)
    - Don't need a compaction waterline if snapshot is locally obtained (just need to guard memtable)
    - Don't need remote broadcasting or RPC server
    - Don't need to block for the server to become the leader.
    - Should prefer to not return rpc::Status to avoid propagating it outwards.

    TODO: do not propsgate RPC errors out of calls to the client.
    */
    pub async fn create_local(dir: &LocalPath) -> Result<TransactionalDBClient> {
        let mut rpc_handler = rpc::Http2RequestHandler::new();

        let rpc_server_ready = Arc::new(Eventually::new());
        rpc_server_ready.set(()).await?;

        let service = Self::create(TransactionalDBOptions {
            dir: dir.to_owned(),
            state_machine: EmbeddedDBStateMachineOptions::default(),
            log: SegmentedLogOptions::default(),
            bootstrap_group: true,
            bootstrap_node_id: Some(1),
            service_port: 0, // Unused. 0 will disable discovery.
            route_labels: vec![],
            hostname_resolver: Arc::new(raft_client::DefaultHostnameResolver::default()),
            tls: None,
            acl_processor: None
        }, &mut rpc_handler, rpc_server_ready).await?;

        let channel = Arc::new(rpc::LocalChannel::from_handler(Arc::new(rpc_handler)));

        let client = TransactionalDBClient::create_local(channel, service).await;

        // TODO: Generalize this and instead use wait_for_ready() on the service.
        for _ in 0..5000 {
            let status = client.current_status().await?;
            if status.role() == raft::proto::Status_Role::LEADER {
                break;
            }

            executor::sleep(Duration::from_millis(1)).await?;
        }

        Ok(client)
    }

}
