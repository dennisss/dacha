use std::collections::{BTreeMap, HashMap};
use std::future::Future;
use std::ops::Bound;
use std::sync::Arc;
use std::time::SystemTime;

use common::async_fn::AsyncFn1;
use common::bytes::Bytes;
use common::errors::*;
use db_kv::KeyValueEntry;
use db_txn_proto::db::txn::*;
use db_table::key_utils::*;
use executor::cancellation::AlreadyCancelledToken;
use executor::child_task::ChildTask;
use executor::sync::{AsyncMutex, AsyncMutexGuard, AsyncMutexPermit};
use executor::{lock, lock_async};
use executor_multitask::{impl_resource_passthrough, ServiceResource, ServiceResourceGroup};
use net::ip::SocketAddr;
use raft_client::proto::RouteLabel;
use raft_client::server::channel_factory::ChannelFactory;
use raft_client::{RouteChannelFactory, RouteStore};

use crate::constants::*;

/// Maximum number of times transactions should be retried if they fail due to conflicting writes.
pub const MAX_TRANSACTION_RETRIES: usize = 5;

/// Client library for talking to transactional db servers to read/write data.
///
/// See the KeyValueStore trait for all available methods.
pub struct TransactionalDBClient {
    /// Main channel in this client which is used to execute requests against
    channel: Arc<dyn rpc::Channel>,

    /// References to the state/layout of the whole metastore server cluster.
    /// Will be None only if we are currently directly to a single node.
    cluster: Option<ClusterState>,

    resources: ServiceResourceGroup,
}

struct ClusterState {
    route_store: RouteStore,
    route_channel_factory: RouteChannelFactory,
}

impl_resource_passthrough!(TransactionalDBClient, resources);

/*
Doing discovery in a GCP instance
- Use SRV records to discover internal servers
- Unless in a cluster worker, then we can rely on cached info

`meta.discovery.[zone].cluster.internal.`

- Configure using Cloud DNS API.

*/

impl TransactionalDBClient {
    /// Creates a new client instance.
    ///
    /// The store servers will automatically be discovered via multicast asyncronously. The
    /// main downside of this is that it may take a few seconds to receive the
    /// next broadcast in order to connect.
    pub async fn create(
        labels: &[RouteLabel],
        seeds: &[String],
        hostname_resolver: Arc<dyn raft_client::RouteHostnameResolver>,
        tls_options: Option<crypto::tls::ClientOptionsContainer>,
    ) -> Result<Self> {
        let route_store = raft_client::RouteStore::new(labels, hostname_resolver);

        let resources = ServiceResourceGroup::new("TransactionalDBClient");

        // TODO: Require the route_store to be initialized for the client to be
        // considered to be healthy.

        // TODO: A risk if that we discover a server that is broadcasting but hasn't
        // joined the raft group yet.
        // - Ideally routes also contain whether or not the server is use-able yet.

        /// TODO: With this approach, it may take us up to 2 seconds (the
        /// broadcast interval) to find a server.
        ///
        /// For a normal container on a machine, we want to have a name
        /// resolution cache.
        /// - A single worker per machine 'system.name_service' service
        ///     - Acts as an RPC based DNS service (handles both cluster and out
        ///       of cluster requests).
        ///     - This means that we can sustain an outage to the metastore so
        ///       long as all needed services are cached.
        /// - If running in a unit test, the facttory
        ///
        /// TODO: Have some level of security for this.
        let discovery = raft_client::DiscoveryMulticast::create(route_store.clone()).await?;
        resources
            .register_dependency(Arc::new(discovery.start()))
            .await;

        if !seeds.is_empty() {
            let client = raft_client::DiscoveryClient::create(
                route_store.clone(),
                raft_client::DiscoveryClientOptions {
                    seeds: seeds.to_vec(),
                    active_broadcaster: false,
                    tls_options: tls_options.clone(),
                },
            )
            .await;
            resources
                .spawn_interruptable("raft::DiscoveryClient", client.run())
                .await;
        }

        // TODO: In the resolver, also subscribe to one of the server's CurrentStatus.

        let channel_factory = raft_client::RouteChannelFactory::new(route_store.clone(), tls_options.clone());

        let channel = channel_factory.create_leader().await?;

        Self::create_impl(
            channel,
            resources,
            Some(ClusterState {
                route_store,
                route_channel_factory: channel_factory,
            }),
        )
        .await
    }

    /// Directly connect to a metastore instance.
    ///
    /// This is mainly for use for testing where we only need to communicate
    /// with a single instance.
    ///
    /// TODO: Restrict to other this and the main crate.
    pub async fn create_direct(addr: SocketAddr) -> Result<Self> {
        let mut options: rpc::Http2ChannelOptions = format!("http://{}", addr.to_string())
            .as_str()
            .try_into_result()?;
        options.base_path = "/rpc".to_string();

        let channel = Arc::new(rpc::Http2Channel::create(options).await?);

        Self::create_impl(channel, ServiceResourceGroup::new("TransactionalDBClient"), None).await
    }

    pub async fn create_local(channel: Arc<dyn rpc::Channel>, resource: Arc<dyn ServiceResource>) -> Self {
        let resources = ServiceResourceGroup::new("TransactionalDBClient");
        resources.register_dependency(resource).await;
        
        Self {
            channel,
            resources,
            cluster: None
        }
    }

    async fn create_impl(
        channel: Arc<rpc::Http2Channel>,
        resources: ServiceResourceGroup,
        cluster: Option<ClusterState>,
    ) -> Result<Self> {
        resources.register_dependency(channel.clone()).await;

        Ok(Self {
            channel,
            resources,
            cluster,
        })
    }

    pub async fn close(self) -> Result<()> {
        self.add_cancellation_token(Arc::new(AlreadyCancelledToken::default()))
            .await;
        self.wait_for_termination().await
    }

    /// List of seed server addresses that can be used later to rediscover the
    /// metastore more quickly.
    pub async fn seeds(&self) -> Vec<String> {
        let mut addrs = self.known_servers().await;
        addrs.truncate(3);
        addrs
    }

    /// Retrieves a list of known server addresses. Can be used to seed a future
    /// TransactionalDBClient instance.
    ///
    /// The list is sorted from most to least likely to be currently healthy.
    async fn known_servers(&self) -> Vec<String> {
        let cluster = match self.cluster.as_ref() {
            Some(v) => v,
            None => return vec![],
        };

        let mut out = vec![];

        let guard = cluster.route_store.lock().await;
        for route in guard.remote_routes() {
            out.push((
                SystemTime::from(route.last_seen()),
                route.target().addr().to_string(),
            ));
        }

        drop(guard);

        // TODO: Also factor in 'ready'
        out.sort();

        out.into_iter().map(|(_, addr)| addr).collect()
    }

    /// Request context to use if we are not running in a transaction.
    fn default_request_context(&self) -> Result<rpc::ClientRequestContext> {
        let mut request_context = rpc::ClientRequestContext::default();
        // TODO: Label this in the protobuf service description?
        request_context.idempotent = true;
        Ok(request_context)
    }

    /// CANCEL SAFE
    async fn get_impl(
        &self,
        key: &[u8],
        transaction_state: Option<AsyncMutexPermit<'_, MetastoreTransactionState>>,
    ) -> Result<Option<Bytes>> {
        let stub = KeyValueStoreStub::new(self.channel.clone());
        let mut request_context = self.default_request_context()?;
        request_context.buffer_full_response = true;

        let mut request = ReadRequest::default();

        let (start_key, end_key) = single_key_range(key);
        request.keys_mut().set_start_key(start_key.as_ref());
        request.keys_mut().set_end_key(end_key.as_ref());

        if let Some(transaction_state_permit) = transaction_state {
            lock!(transaction_state <= transaction_state_permit, {
                request.set_read_index(transaction_state.read_index);

                // TODO: Ensure the ranges are all non-overlapping.
                transaction_state.reads.push(request.keys().clone());
            });
        }

        let mut response = stub.Read(&request_context, &request).await;
        let value = if let Some(res) = response.recv().await {
            if !res.entry().deleted() {
                Some(res.entry().value().into())
            } else {
                None
            }
        } else {
            None
        };

        if response.recv().await.is_some() {
            return Err(err_msg("Received multiple values"));
        }

        response.finish().await?;

        Ok(value)
    }

    /// Lists all files in a directory (along with their contents.)
    ///
    /// CANCEL-SAFE
    async fn get_range_impl(
        &self,
        start_key: &[u8],
        end_key: &[u8],
        transaction_state_permit: Option<AsyncMutexPermit<'_, MetastoreTransactionState>>,
    ) -> Result<Vec<KeyValueEntryProto>> {
        let stub = KeyValueStoreStub::new(self.channel.clone());
        let mut request_context = self.default_request_context()?;
        request_context.buffer_full_response = true;

        let mut request = ReadRequest::default();

        request.keys_mut().set_start_key(start_key);
        request.keys_mut().set_end_key(end_key);

        // TODO: Deduplicate this code.
        if let Some(transaction_state_permit) = transaction_state_permit {
            lock!(transaction_state <= transaction_state_permit, {
                request.set_read_index(transaction_state.read_index);
                transaction_state.reads.push(request.keys().clone());
            });
        }

        let mut out = vec![];

        let mut response = stub.Read(&request_context, &request).await;
        while let Some(res) = response.recv().await {
            out.push(res.entry().clone());
        }

        response.finish().await?;

        Ok(out)
    }

    async fn put_impl(&self, key: &[u8], value: &[u8]) -> Result<()> {
        let stub = KeyValueStoreStub::new(self.channel.clone());
        let request_context = self.default_request_context()?;

        let mut request = ExecuteRequest::default();
        let mut op = Operation::default();
        op.set_key(key);
        op.set_put(value);
        request.transaction_mut().add_writes(op);

        stub.Execute(&request_context, &request).await.result?;
        Ok(())
    }

    async fn delete_impl(&self, key: &[u8]) -> Result<()> {
        let stub = KeyValueStoreStub::new(self.channel.clone());
        let request_context = self.default_request_context()?;

        let mut request = ExecuteRequest::default();
        let mut op = Operation::default();
        op.set_key(key);
        op.set_delete(true);
        request.transaction_mut().add_writes(op);

        stub.Execute(&request_context, &request).await.result?;
        Ok(())
    }

    async fn new_transaction_impl<'a>(&'a self) -> Result<MetastoreTransaction<'a>> {
        let stub = KeyValueStoreStub::new(self.channel.clone());

        let mut request = SnapshotRequest::default();
        request.set_latest(true);
        request.set_optimistic(true); // Safe as this will be checked later during commit.

        let res = stub
            .Snapshot(&self.default_request_context()?, &request)
            .await
            .result?;

        Ok(MetastoreTransaction {
            class: MetastoreTransactionClass::TopLevel {
                client: self,
                state: AsyncMutex::new(MetastoreTransactionState {
                    read_index: res.read_index(),
                    reads: Vec::new(),
                    writes: BTreeMap::new(),
                }),
            },
        })
    }

    /// NOTE: Once this returns, all future changess creates by any other client
    /// will be acounted for.
    ///
    /// TODO: Need higher level logic in here for retrying watch failures on new
    /// leaders when this fails.
    pub async fn watch(&self, key_prefix: &str) -> Result<WatchStream> {
        let stub = KeyValueStoreStub::new(self.channel.clone());
        let request_context = self.default_request_context()?;

        let mut request = WatchRequest::default();
        request.set_key_prefix(key_prefix.as_bytes());

        let mut response = stub.Watch(&request_context, &request).await;

        // TODO:
        response.recv_head().await;

        Ok(WatchStream { response })
    }

    pub async fn current_status(&self) -> Result<raft_client::proto::Status> {
        let stub = ServerManagementStub::new(self.channel.clone());
        let request_context = self.default_request_context()?;

        let request = protobuf_builtins::google::protobuf::Empty::default();
        stub.CurrentStatus(&request_context, &request).await.result
    }

    pub async fn remove_server(&self, id: raft_client::proto::ServerId) -> Result<()> {
        let request_context = self.default_request_context()?;

        // TODO: Eventually undrain.
        {
            let cluster = self.cluster.as_ref().ok_or_else(|| {
                err_msg("Must use a cluster wide TransactionalDBClient to drain servers")
            })?;

            let stub = ServerManagementStub::new(cluster.route_channel_factory.create(id).await?);

            let mut request = DrainRequest::default();
            request.set_server_id(id);
            stub.Drain(&request_context, &request).await.result?;
        }

        {
            let stub = ServerManagementStub::new(self.channel.clone());

            let mut request = ConfigChangeRequest::default();
            request.set_remove_server(id);
            stub.ConfigChange(&request_context, &request).await.result?;
        }

        Ok(())
    }
}

// Helpers
impl TransactionalDBClient {
    pub async fn get(&self, key: &[u8]) -> Result<Option<Bytes>> {
        self.get_impl(key, None).await
    }

    pub async fn get_range(&self, start_key: &[u8], end_key: &[u8]) -> Result<Vec<KeyValueEntry>> {
        let items = self.get_range_impl(start_key, end_key, None).await?;

        Ok(items.into_iter().map(|v| {
            db_kv::KeyValueEntry {
                key: v.key().into(),
                value: v.value().into(),
            }
        }).collect())
    }

    pub async fn get_prefix(&self, prefix: &[u8]) -> Result<Vec<KeyValueEntry>> {
        let (start_key, end_key) = prefix_key_range(prefix);
        self.get_range(&start_key, &end_key).await
    }

    pub async fn put(&self, key: &[u8], value: &[u8]) -> Result<()> {
        self.put_impl(key, value).await
    }

    pub async fn delete(&self, key: &[u8]) -> Result<()> {
        self.delete_impl(key).await
    }
}

#[async_trait]
impl db_kv::KeyValueStore for TransactionalDBClient {
    async fn new_transaction<'a>(
        &'a self,
    ) -> Result<Box<dyn db_kv::KeyValueStoreTransaction + 'a>> {
        // TODO: Optimize. Only need to check the snapshot if we do reads.
        let txn = self.new_transaction_impl().await?;
        Ok(Box::new(txn))
    }
}

pub struct MetastoreTransaction<'a> {
    class: MetastoreTransactionClass<'a>,
}

struct MetastoreTransactionState {
    // TODO: Implement this field to ensure we never double commit sub or whole transaction.
    // commited: bool,

    read_index: u64,
    reads: Vec<KeyRange>,
    writes: BTreeMap<Bytes, Operation>,
}

enum MetastoreTransactionClass<'a> {
    TopLevel {
        client: &'a TransactionalDBClient,
        state: AsyncMutex<MetastoreTransactionState>,
    },
    /// A transaction that was started inside of another transaction. This is
    /// just a reference to the top level transaction.
    ///
    /// Committing a nested transaction is a no-op as it is instead committed
    /// later as part of the root transaction.
    Nested {
        client: &'a TransactionalDBClient,
        state: &'a AsyncMutex<MetastoreTransactionState>,
    },
}

#[async_trait]
impl<'a> db_kv::KeyValueStoreTransaction for MetastoreTransaction<'a> {
    async fn get(&self, key: &[u8]) -> Result<Option<Bytes>> {
        self.get_impl(key).await
    }

    async fn iter<'b>(
        &'b self,
        options: db_kv::KeyValueIteratorOptions,
    ) -> Result<Box<dyn db_kv::KeyValueStoreIterator + 'b>> {
        let values = self
            .get_range_impl(&options.start_key, &options.end_key)
            .await?;
        Ok(Box::new(VecIterator {
            entries: values,
            i: 0,
        }))
    }

    async fn read_index(&self) -> u64 {
        let (_, state) = self.get_top_level().await;
        state.read_exclusive().read_index
    }

    async fn put(&mut self, key: &[u8], value: &[u8]) -> Result<()> {
        self.put_impl(key, value).await
    }

    async fn delete(&mut self, key: &[u8]) -> Result<()> {
        self.delete_impl(key).await
    }

    async fn commit(&mut self) -> Result<()> {
        self.commit_impl().await
    }
}

struct VecIterator {
    entries: Vec<KeyValueEntryProto>,
    i: usize,
}

#[async_trait]
impl db_kv::KeyValueStoreIterator for VecIterator {
    async fn next(&mut self) -> Result<Option<KeyValueEntry>> {
        if self.i < self.entries.len() {
            let v = &self.entries[self.i];
            self.i += 1;
            // TODO: Ideally avoid copying here.
            return Ok(Some(db_kv::KeyValueEntry {
                key: v.key().into(),
                value: v.value().into(),
            }));
        }

        Ok(None)
    }
}

impl<'a> MetastoreTransaction<'a> {
    pub async fn read_index(&self) -> u64 {
        let (_, state) = self.get_top_level().await;
        state.read_exclusive().read_index
    }

    async fn get_top_level<'b>(
        &'b self,
    ) -> (
        &'b TransactionalDBClient,
        AsyncMutexPermit<'b, MetastoreTransactionState>,
    ) {
        match &self.class {
            MetastoreTransactionClass::TopLevel { client, state } => {
                (*client, state.lock().await.unwrap())
            }
            MetastoreTransactionClass::Nested { client, state } => {
                (*client, state.lock().await.unwrap())
            }
        }
    }

    /// CANCEL SAFE
    async fn get_impl(&self, key: &[u8]) -> Result<Option<Bytes>> {
        let (client, state_permit) = self.get_top_level().await;

        let state = state_permit.read_exclusive();

        if let Some(op) = state.writes.get(key) {
            match op.typ_case() {
                OperationTypeCase::Put(value) => {
                    return Ok(Some(value.as_ref().into()));
                }
                OperationTypeCase::Delete(_) => {
                    return Ok(None);
                }
                OperationTypeCase::NOT_SET => {}
            }
        }

        client.get_impl(key, Some(state.upgrade())).await
    }

    /// CANCEL SAFE
    async fn get_range_impl(&self, start_key: &[u8], end_key: &[u8]) -> Result<Vec<KeyValueEntryProto>> {
        let (client, state_permit) = self.get_top_level().await;

        self.get_range_with_lock(start_key, end_key, client, state_permit)
            .await
    }

    /// CANCEL SAFE
    async fn get_range_with_lock(
        &self,
        start_key: &[u8],
        end_key: &[u8],
        client: &TransactionalDBClient,
        state_permit: AsyncMutexPermit<'_, MetastoreTransactionState>,
    ) -> Result<Vec<KeyValueEntryProto>> {
        let state = state_permit.read_exclusive();

        let written_values = {
            let mut out = vec![];
            for (_, op) in state
                .writes
                .range::<[u8], _>((Bound::Included(start_key), Bound::Excluded(end_key)))
            {
                let mut entry = KeyValueEntryProto::default();
                entry.set_key(op.key());

                match op.typ_case() {
                    OperationTypeCase::Put(value) => {
                        entry.set_value(value.as_ref());
                    }
                    OperationTypeCase::Delete(_) | OperationTypeCase::NOT_SET => {
                        entry.set_deleted(true);
                    }
                }

                out.push(entry);
            }

            out
        };

        // NOTE: These will always be returned by the server in sorted order.
        // TODO: Support caching this.
        let snapshot_values = client
            .get_range_impl(start_key, end_key, Some(state.upgrade()))
            .await?;

        // Merge preferring the new written_values
        let merged = common::algorithms::merge_by(written_values, snapshot_values, |a, b| {
            a.key().cmp(b.key())
        });

        // Remove deleted ones.
        let combined = merged.into_iter().filter(|v| !v.deleted()).collect();

        Ok(combined)
    }

    async fn put_impl(&self, key: &[u8], value: &[u8]) -> Result<()> {
        let (_, state_permit) = self.get_top_level().await;

        lock!(state <= state_permit, {
            let mut op = Operation::default();
            op.set_key(key);
            op.set_put(value);
            state.writes.insert(key.into(), op);
        });

        Ok(())
    }

    async fn delete_impl(&self, key: &[u8]) -> Result<()> {
        let (_, state_permit) = self.get_top_level().await;

        lock!(state <= state_permit, {
            let mut op = Operation::default();
            op.set_key(key);
            op.set_delete(true);
            state.writes.insert(key.into(), op);
        });

        Ok(())
    }

    async fn new_transaction_impl<'b>(&'b self) -> Result<MetastoreTransaction<'b>> {
        Ok(match &self.class {
            MetastoreTransactionClass::TopLevel { client, state } => MetastoreTransaction {
                class: MetastoreTransactionClass::Nested {
                    client: *client,
                    state,
                },
            },
            MetastoreTransactionClass::Nested { client, state } => MetastoreTransaction {
                class: MetastoreTransactionClass::Nested {
                    client: *client,
                    state: *state,
                },
            },
        })
    }

    pub async fn commit(self) -> Result<()> {
        self.commit_impl().await
    }

    async fn commit_impl(&self) -> Result<()> {
        // Nested transactions will be committed once the top level transaction is
        // committed.
        if let MetastoreTransactionClass::Nested { .. } = self.class {
            return Ok(());
        }

        let (client, state_permit) = self.get_top_level().await;

        lock_async!(state <= state_permit, {
            if state.writes.is_empty() {
                return Ok(());
            }

            let mut request = ExecuteRequest::default();
            request.transaction_mut().set_read_index(state.read_index);

            for read in &state.reads {
                request.transaction_mut().add_reads(read.clone());
            }

            // NOTE: The keys should have already been added to each operation.
            for (_, op) in state.writes.iter() {
                request.transaction_mut().add_writes(op.clone());
            }

            let stub = KeyValueStoreStub::new(client.channel.clone());
            let request_context = client.default_request_context()?;
            stub.Execute(&request_context, &request).await.result?;

            Ok(())
        })
    }
}

pub struct WatchStream {
    response: rpc::ClientStreamingResponse<WatchResponse>,
}

// TODO: Improve this (need to specifically check for Aborted errors).
/// TODO: This needs to detect retryable/cancellation related errors.
#[macro_export]
macro_rules! run_transaction {
    ($client:expr, $txn:ident, $f:expr) => {{
        let mut retval = None;
        for i in 0..$crate::MAX_TRANSACTION_RETRIES {
            let mut $txn = $client.new_transaction().await?;
            retval = Some($f);
            $txn.commit().await?;
            break;
        }

        retval.ok_or_else(|| err_msg("Transaction exceeded max number of retries"))?
    }};
}
