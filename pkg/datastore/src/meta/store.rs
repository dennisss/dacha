use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::sync::atomic::AtomicUsize;
use std::sync::Arc;

use common::async_std::channel;
use common::async_std::path::{Path, PathBuf};
use common::async_std::sync::Mutex;
use common::async_std::task;
use common::bundle::TaskResultBundle;
use common::bytes::Bytes;
use common::errors::*;
use common::fs::DirLock;
use common::task::ChildTask;
use protobuf::Message;
use raft::atomic::{BlobFile, BlobFileBuilder};
use raft::StateMachine;
use rpc_util::AddReflection;
use sstable::db::{SnapshotIteratorOptions, WriteBatch};
use sstable::iterable::Iterable;

use crate::meta::constants::*;
use crate::meta::state_machine::*;
use crate::meta::table_key::TableKey;
use crate::meta::transaction::*;
use crate::proto::client::*;
use crate::proto::key_value::*;
use crate::proto::meta::UserDataSubKey;

/*
Performing transaction checking in the metastore:
- makes it easier to parallelize in the future
- if we can get the sequence number, then that could bebest

*/

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

pub struct MetastoreConfig {
    /// Path to the directory used to store all of the store's data (at least
    /// this machine's copy).
    pub dir: PathBuf,

    /// Port used for listening for RPC signals for bootstrapping this server in
    /// a new cluster.
    ///
    /// Not used for already setup clusters.
    pub init_port: u16,

    /// Server port of the RPC service exposed to users of the store.
    /// This will also be used for internal communication between servers.
    pub service_port: u16,
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

/*
Long term:
- Use the raft log index as the revision
- perform TTLing of

- We should be able to determine what is the highest compacted sequence.

-> Can only remove a key if the overwrite of that key was more than 24 hours ago
*/

impl Metastore {
    fn get_client_id<T: protobuf::Message>(request: &rpc::ServerRequest<T>) -> Result<&str> {
        match request.context.metadata.get_text(CLIENT_ID_KEY) {
            Ok(Some(v)) => Ok(v),
            _ => Err(rpc::Status::invalid_argument(
                "Invalid or missing client id in request context",
            )
            .into()),
        }
    }

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
            .await?;

        response.set_read_index(read_index.index().value());

        Ok(())
    }

    async fn read_impl<'a>(
        &self,
        request: rpc::ServerRequest<ReadRequest>,
        response: &mut rpc::ServerStreamResponse<'a, ReadResponse>,
    ) -> Result<()> {
        self.shared
            .node
            .server()
            .begin_read(request.read_index() != 0)
            .await?;

        // TODO: Support changing to a specific read_index (will require checking the
        // flush index).
        let snapshot = self.shared.state_machine.snapshot().await;

        let start_key = TableKey::user_value(request.keys().start_key());
        let end_key = TableKey::user_value(request.keys().end_key());

        let mut iter_options = SnapshotIteratorOptions::default();
        if request.read_index() > 0 {
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

            let mut res = ReadResponse::default();
            res.entry_mut().set_key(&user_key[..]);

            if let Some(value) = entry.value {
                res.entry_mut().set_value(value.as_ref());
            } else {
                res.entry_mut().set_deleted(true);
            }

            response.send(res).await?;
        }

        Ok(())
    }

    /*
    Performing conflict analysis on the server:
    - Pros:
        - Simplifies the WAL format
        - More easy to parallelize
    - Cons:
        - The naive implementation will not allow concurrent executions (as the state machine won't get updated until )

    - Need light weight list of writer locks.
        => Must be able to lock all desired keys.

    Cons of this:
    - If there are conflicts, it will take an entire
    */

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
            let mut internal_op = op.clone();
            internal_op.set_key(TableKey::user_value(op.key()));
            internal_txn.add_writes(internal_op);
        }

        let index = self
            .shared
            .transaction_manager
            .execute(
                &internal_txn,
                self.shared.node.clone(),
                &self.shared.state_machine,
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
        response: &mut rpc::ServerStreamResponse<'a, KeyValueEntry>,
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

        loop {
            let kv = registration.recv().await?;
            response.send(kv).await?;
        }
    }

    async fn new_unique_id(&self) -> Result<String> {
        // If this succeeds, then we know that we were the leader in the given term.
        // If we have a locally unique value, we can make it globally unique by
        // prepending this term.
        let term = self.shared.node.server().begin_read(true).await?.term();

        let index = self
            .shared
            .next_local_id
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);

        Ok(format!("{}:{}", term.value(), index))
    }
}

#[async_trait]
impl ClientManagementService for Metastore {
    async fn NewClient(
        &self,
        request: rpc::ServerRequest<google::proto::empty::Empty>,
        response: &mut rpc::ServerResponse<NewClientResponse>,
    ) -> Result<()> {
        response.value.set_client_id(self.new_unique_id().await?);
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
        response: &mut rpc::ServerStreamResponse<KeyValueEntry>,
    ) -> Result<()> {
        self.watch_impl(request, response).await
    }
}

pub async fn run(config: &MetastoreConfig) -> Result<()> {
    if !config.dir.exists().await {
        common::async_std::fs::create_dir(&config.dir).await?;
    }

    let dir = DirLock::open(&config.dir).await?;
    let state_machine = Arc::new(EmbeddedDBStateMachine::open(&config.dir).await?);

    let mut task_bundle = TaskResultBundle::new();

    let mut rpc_server = rpc::Http2Server::new();

    let mut node = raft::Node::create(raft::NodeOptions {
        dir,
        init_port: config.init_port,
        bootstrap: false,
        seed_list: vec![], // Will just find everyone via multi-cast
        state_machine: state_machine.clone(),
        last_applied: state_machine.last_flushed().await,
    })
    .await?;

    let local_address = http::uri::Authority {
        user: None,
        host: http::uri::Host::IP(net::local_ip()?),
        port: Some(config.service_port),
    }
    .to_string()?;

    task_bundle.add("raft::Node", node.run(&mut rpc_server, &local_address)?);

    let node = Arc::new(node);

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

    rpc_server.add_reflection()?;

    task_bundle.add("rpc::Server", rpc_server.run(config.service_port));

    task_bundle.join().await?;

    Ok(())
}
