use std::collections::HashSet;
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
use protobuf::Message;
use raft::atomic::{BlobFile, BlobFileBuilder};
use raft::StateMachine;
use rpc_util::AddReflection;
use sstable::iterable::Iterable;
use sstable::{EmbeddedDB, EmbeddedDBOptions};

use crate::meta::state_machine::*;
use crate::meta::table_key::TableKey;
use crate::proto::client::*;
use crate::proto::key_value::*;
use crate::proto::meta::UserDataSubKey;

use super::constants::CLIENT_ID_KEY;

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

    state: Mutex<MetastoreState>,

    watchers: Arc<Mutex<WatchersState>>,

    next_client_index: AtomicUsize,

    /// NOTE: Only reads go through this object. Writes must go through the
    /// replication_Server.
    state_machine: Arc<EmbeddedDBStateMachine>,
}

struct MetastoreState {
    // Should also keep track of actively leased out transactions.
    // transactions should die
    /// Set of keys which are currently locked by some transaction that is
    /// currently getting comitted.
    key_locks: HashSet<Bytes>,
    /* List of transactios
     * - Die after a TTL
     * - Need to know client id so that we can check for leases. */
}

struct WatchersState {
    // TODO: Use a BTreeMap
    prefix_watchers: Vec<WatcherEntry>,

    last_id: usize,
}

struct WatcherEntry {
    key_prefix: Bytes,
    id: usize,
    client_id: String,
    sender: channel::Sender<KeyValue>,
}

struct WatcherRegistration {
    state: Arc<Mutex<WatchersState>>,
    id: usize,
    receiver: channel::Receiver<KeyValue>,
}

impl Drop for WatcherRegistration {
    fn drop(&mut self) {
        let state = self.state.clone();
        let id = self.id;
        task::spawn(async move {
            let mut state = state.lock().await;
            for i in 0..state.prefix_watchers.len() {
                if state.prefix_watchers[i].id == id {
                    state.prefix_watchers.swap_remove(i);
                    break;
                }
            }
        });
    }
}

/*
TODO: If the RouteChannel doesn't know the leader tempoarily, we need to support retrying the RPCs internally.

- Routestore

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

impl Metastore {
    /*
    pub fn start_transaction(&self) {}

    pub async fn commit_transaction(&self, transaction: Transaction) {
        // Acquire locks for all keys (make sure that this can bail out)
        // - Note: We may need to issue a cleanup task::spawn() if the task
        //   isn't polled
    }
    */

    fn get_client_id<T: protobuf::Message>(request: &rpc::ServerRequest<T>) -> Result<&str> {
        match request.context.metadata.get_text(CLIENT_ID_KEY) {
            Ok(Some(v)) => Ok(v),
            _ => Err(rpc::Status::invalid_argument(
                "Invalid or missing client id in request context",
            )
            .into()),
        }
    }

    async fn get(&self, request: rpc::ServerRequest<Key>, response: &mut KeyValue) -> Result<()> {
        self.shared.node.server().begin_read(false).await?;

        let db = self.shared.state_machine.db();
        // let snapshot = db.snapshot().await;

        let table_key = TableKey::user_value(request.data());

        let value = db
            .get(&table_key)
            .await?
            .ok_or_else(|| rpc::Status::not_found("Key doesn't exist"))?;

        response.set_key(request.data());
        response.set_value(value.as_ref());
        Ok(())
    }

    async fn get_range<'a>(
        &self,
        request: rpc::ServerRequest<KeyRange>,
        response: &mut rpc::ServerStreamResponse<'a, KeyValue>,
    ) -> Result<()> {
        self.shared.node.server().begin_read(false).await?;

        let db = self.shared.state_machine.db();
        let snapshot = db.snapshot().await;

        let start_key = TableKey::user_value(request.start_key());
        let end_key = TableKey::user_value(request.end_key());

        let mut iter = snapshot.iter().await;
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

            let mut res = KeyValue::default();
            res.set_key(&user_key[..]);
            res.set_value(entry.value.as_ref());
            response.send(res).await?;
        }

        Ok(())
    }

    async fn put(&self, request: rpc::ServerRequest<KeyValue>) -> Result<()> {
        let client_id = Self::get_client_id(&request)?;

        let table_key = TableKey::user_value(request.key());

        let mut write = sstable::db::WriteBatch::new();
        write.put(&table_key, request.value());
        self.shared
            .node
            .server()
            .execute(write.as_bytes().to_vec())
            .await?;

        let state = self.shared.watchers.lock().await;
        for watcher in &state.prefix_watchers {
            if &watcher.client_id == client_id {
                continue;
            }

            if request.key().starts_with(watcher.key_prefix.as_ref()) {
                // NOTE: To prevent blocking the write path, this must use an unbounded channel.
                let _ = watcher.sender.send(request.clone()).await;
            }
        }

        Ok(())
    }

    // TODO: This can be implemented on any follower server if we pull changes from
    // the state machine.
    //
    // TODO: Support ignoring changes from the same client as the one that initiated
    // the watch?
    async fn watch<'a>(
        &self,
        request: rpc::ServerRequest<WatchRequest>,
        response: &mut rpc::ServerStreamResponse<'a, KeyValue>,
    ) -> Result<()> {
        let client_id = Self::get_client_id(&request)?;

        let registration = {
            let mut state = self.shared.watchers.lock().await;

            let id = state.last_id + 1;
            state.last_id = id;

            let (sender, receiver) = channel::unbounded();

            let entry = WatcherEntry {
                key_prefix: Bytes::from(request.key_prefix()),
                client_id: client_id.to_string(),
                id,
                sender,
            };

            // NOTE: These two lines must happen atomically to ensure that the entry is
            // always cleaned up.
            state.prefix_watchers.push(entry);
            WatcherRegistration {
                state: self.shared.watchers.clone(),
                id,
                receiver,
            }
        };

        // Send head so that the client can properly syncronize the time at which
        // watching starts.
        response.send_head().await?;

        loop {
            let kv = registration.receiver.recv().await?;
            response.send(kv).await?;
        }
    }

    async fn new_client(&self) -> Result<String> {
        // If this succeeds, then we know that we were the leader in the given term.
        // If we have a locally unique value, we can make it globally unique by
        // prepending this term.
        let term = self.shared.node.server().begin_read(true).await?.term();

        let index = self
            .shared
            .next_client_index
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
        response.value.set_client_id(self.new_client().await?);
        Ok(())
    }
}

#[async_trait]
impl KeyValueStoreService for Metastore {
    async fn Get(
        &self,
        request: rpc::ServerRequest<Key>,
        response: &mut rpc::ServerResponse<KeyValue>,
    ) -> Result<()> {
        self.get(request, &mut response.value).await
    }

    async fn GetRange(
        &self,
        request: rpc::ServerRequest<KeyRange>,
        response: &mut rpc::ServerStreamResponse<KeyValue>,
    ) -> Result<()> {
        self.get_range(request, response).await
    }

    async fn Put(
        &self,
        request: rpc::ServerRequest<KeyValue>,
        response: &mut rpc::ServerResponse<google::proto::empty::Empty>,
    ) -> Result<()> {
        self.put(request).await
    }

    async fn Delete(
        &self,
        request: rpc::ServerRequest<Key>,
        response: &mut rpc::ServerResponse<google::proto::empty::Empty>,
    ) -> Result<()> {
        Err(err_msg("Not implemented"))
    }

    async fn Watch(
        &self,
        request: rpc::ServerRequest<WatchRequest>,
        response: &mut rpc::ServerStreamResponse<KeyValue>,
    ) -> Result<()> {
        self.watch(request, response).await
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
            watchers: Arc::new(Mutex::new(WatchersState {
                prefix_watchers: vec![],
                last_id: 0,
            })),
            state: Mutex::new(MetastoreState {
                key_locks: HashSet::new(),
            }),
            next_client_index: AtomicUsize::new(1),
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
