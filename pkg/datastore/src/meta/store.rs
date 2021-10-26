use std::collections::HashSet;
use std::sync::Arc;

use common::async_std::path::{Path, PathBuf};
use common::async_std::sync::Mutex;
use common::bundle::TaskResultBundle;
use common::bytes::Bytes;
use common::errors::*;
use common::fs::DirLock;
use protobuf::Message;
use raft::atomic::{BlobFile, BlobFileBuilder};
use raft::StateMachine;
use rpc_util::AddReflection;
use sstable::{EmbeddedDB, EmbeddedDBOptions};

use crate::meta::state_machine::*;
use crate::proto::key_value::*;

pub struct MetaStoreConfig {
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

pub struct MetaStore {
    node: Arc<raft::Node<()>>,

    state: Mutex<MetaStoreState>,

    /// NOTE: Only reads go through this object. Writes must go through the
    /// replication_Server.
    state_machine: Arc<EmbeddedDBStateMachine>,
}

struct MetaStoreState {
    // Should also keep track of actively leased out transactions.
    // transactions should die
    /// Set of keys which are currently locked by some transaction that is
    /// currently getting comitted.
    key_locks: HashSet<Bytes>,
}

/*
TODO: If the RouteChannel doesn't know the leader tempoarily, we need to support retrying the RPCs internally.

- Routestore

Need an event listener on the Server to tell when we become a leader vs. stop being the leader
- If we are not the leader, we need to cancel all transactions.

Limits on transactions:
- max lifetime: 10 seconds

- We are either the leader or we have a

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

impl MetaStore {
    /*
    pub fn start_transaction(&self) {}

    pub async fn commit_transaction(&self, transaction: Transaction) {
        // Acquire locks for all keys (make sure that this can bail out)
        // - Note: We may need to issue a cleanup task::spawn() if the task
        //   isn't polled
    }
    */

    async fn get(&self, request: &Key, response: &mut KeyValue) -> Result<()> {
        self.node.server().begin_read(false).await?;

        let db = self.state_machine.db();
        // let snapshot = db.snapshot().await;

        let value = db
            .get(request.data())
            .await?
            .ok_or_else(|| rpc::Status::not_found("Key doesn't exist"))?;

        response.set_key(request.data());
        response.set_value(value.as_ref());
        Ok(())
    }

    async fn put(&self, request: &KeyValue) -> Result<()> {
        let mut write = sstable::db::WriteBatch::new();
        write.put(request.key(), request.value());
        self.node
            .server()
            .execute(write.as_bytes().to_vec())
            .await?;
        Ok(())
    }
}

#[async_trait]
impl KeyValueStoreService for MetaStore {
    async fn Get(
        &self,
        request: rpc::ServerRequest<Key>,
        response: &mut rpc::ServerResponse<KeyValue>,
    ) -> Result<()> {
        self.get(&request.value, &mut response.value).await
    }

    async fn GetRange(
        &self,
        request: rpc::ServerRequest<KeyRange>,
        response: &mut rpc::ServerStreamResponse<KeyValue>,
    ) -> Result<()> {
        Ok(())
    }

    async fn Put(
        &self,
        request: rpc::ServerRequest<KeyValue>,
        response: &mut rpc::ServerResponse<google::proto::empty::Empty>,
    ) -> Result<()> {
        self.put(&request.value).await
    }

    async fn Delete(
        &self,
        request: rpc::ServerRequest<Key>,
        response: &mut rpc::ServerResponse<google::proto::empty::Empty>,
    ) -> Result<()> {
        Ok(())
    }
}

pub async fn run(config: &MetaStoreConfig) -> Result<()> {
    if !config.dir.exists().await {
        common::async_std::fs::create_dir(&config.dir).await;
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

    task_bundle.add(
        "raft::Node",
        node.run(
            &mut rpc_server,
            &format!("http://127.0.0.1:{}", config.service_port),
        )?,
    );

    let node = Arc::new(node);

    let local_service = MetaStore {
        node: node.clone(),
        state_machine,
        state: Mutex::new(MetaStoreState {
            key_locks: HashSet::new(),
        }),
    }
    .into_service();

    rpc_server.add_service(Arc::new(raft::LeaderServiceWrapper::new(
        node.clone(),
        local_service,
    )))?;

    rpc_server.add_reflection()?;

    task_bundle.add("rpc::Server", rpc_server.run(config.service_port));

    task_bundle.join().await?;

    Ok(())
}
