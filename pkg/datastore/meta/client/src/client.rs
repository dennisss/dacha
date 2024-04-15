use std::collections::{BTreeMap, HashMap};
use std::future::Future;
use std::ops::Bound;
use std::sync::Arc;

use common::async_fn::AsyncFn1;
use common::bytes::Bytes;
use common::errors::*;
use datastore_proto::db::meta::*;
use executor::cancellation::AlreadyCancelledToken;
use executor::child_task::ChildTask;
use executor::sync::{AsyncMutex, AsyncMutexGuard, AsyncMutexPermit};
use executor::{lock, lock_async};
use executor_multitask::{impl_resource_passthrough, ServiceResource, ServiceResourceGroup};
use net::ip::SocketAddr;
use raft_client::proto::RouteLabel;

use crate::constants::*;
use crate::key_utils::*;

/// Maximum number of times metastore transactions should be retried if
pub const MAX_TRANSACTION_RETRIES: usize = 5;

/// Client library for talking to metastore servers to read/write data.
///
/// See the MetastoreClientInterface trait for all available methods.
pub struct MetastoreClient {
    client_id: String,

    channel: Arc<dyn rpc::Channel>,

    resources: ServiceResourceGroup,
}

impl_resource_passthrough!(MetastoreClient, resources);

/*
Doing discovery in a GCP instance
- Use SRV records to discover internal servers
- Unless in a cluster worker, then we can rely on cached info

`meta.discovery.[zone].cluster.internal.`

- Configure using Cloud DNS API.

*/

impl MetastoreClient {
    /// Creates a new client instance.
    ///
    /// The store servers will automatically be discovered via multicast. The
    /// main downside of this is that it may take a few seconds to receive the
    /// next broadcast in order to connect.
    pub async fn create(labels: &[RouteLabel]) -> Result<Self> {
        let route_store = raft_client::RouteStore::new(labels);

        let resources = ServiceResourceGroup::new("MetastoreClient");

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
        let discovery = raft_client::DiscoveryMulticast::create(route_store.clone()).await?;
        resources
            .register_dependency(Arc::new(discovery.start()))
            .await;

        // TODO: In the resolver, also subscribe to one of the server's CurrentStatus.
        // Whenever the set of members changes, use that info to prune the routes we
        // have on the client side.
        let channel_factory =
            raft_client::RouteChannelFactory::find_group(route_store.clone()).await;

        // We can talk to any metastore worker as they will all redirect requests to the
        // leader if needed.
        let channel = channel_factory.create_any().await?;

        Self::create_impl(channel, resources).await
    }

    /// Directly connect to a metastore instance.
    ///
    /// This is mainly for use for testing where we only need to communicate
    /// with a single instance.
    ///
    /// TODO: Restrict to other this and the main crate.
    pub async fn create_direct(addr: SocketAddr) -> Result<Self> {
        let channel = Arc::new(
            rpc::Http2Channel::create(format!("http://{}", addr.to_string()).as_str()).await?,
        );

        Self::create_impl(channel, ServiceResourceGroup::new("MetastoreClient")).await
    }

    async fn create_impl(
        channel: Arc<rpc::Http2Channel>,
        resources: ServiceResourceGroup,
    ) -> Result<Self> {
        resources.register_dependency(channel.clone()).await;

        // TODO: Make this asyncronous?
        let client_id = {
            let stub = ClientManagementStub::new(channel.clone());

            let req = protobuf_builtins::google::protobuf::Empty::default();
            let mut ctx = rpc::ClientRequestContext::default();
            ctx.http.wait_for_ready = true;
            let res = stub.NewClient(&ctx, &req).await;

            res.result?.client_id().to_string()
        };

        Ok(Self {
            client_id,
            channel,
            resources,
        })
    }

    pub async fn close(self) -> Result<()> {
        self.add_cancellation_token(Arc::new(AlreadyCancelledToken::default()))
            .await;
        self.wait_for_termination().await
    }

    /// Request context to use if we are not running in a transaction.
    fn default_request_context(&self) -> Result<rpc::ClientRequestContext> {
        let mut request_context = rpc::ClientRequestContext::default();
        request_context
            .metadata
            .add_text(CLIENT_ID_KEY, &self.client_id)?;
        Ok(request_context)
    }

    /// CANCEL SAFE
    async fn get_impl(
        &self,
        key: &[u8],
        transaction_state: Option<AsyncMutexPermit<'_, MetastoreTransactionState>>,
    ) -> Result<Option<Vec<u8>>> {
        let stub = KeyValueStoreStub::new(self.channel.clone());
        let request_context = self.default_request_context()?;

        let mut request = ReadRequest::default();

        let (start_key, end_key) = single_key_range(key);
        request.keys_mut().set_start_key(start_key.as_ref());
        request.keys_mut().set_end_key(end_key.as_ref());

        if let Some(transaction_state_permit) = transaction_state {
            lock!(transaction_state <= transaction_state_permit, {
                request.set_read_index(transaction_state.read_index);
                transaction_state.reads.push(request.keys().clone());
            });
        }

        let mut response = stub.Read(&request_context, &request).await;
        let value = if let Some(res) = response.recv().await {
            if !res.entry().deleted() {
                Some(res.entry().value().to_vec())
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
    ) -> Result<Vec<KeyValueEntry>> {
        let stub = KeyValueStoreStub::new(self.channel.clone());
        let request_context = self.default_request_context()?;

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
        let stub = ServerManagementStub::new(self.channel.clone());
        let request_context = self.default_request_context()?;

        let mut request = ConfigChangeRequest::default();
        request.set_remove_server(id);
        stub.ConfigChange(&request_context, &request).await.result?;
        Ok(())
    }
}

//// Interface for interacting with the metastore's key-value file system.
#[async_trait]
pub trait MetastoreClientInterface: Send + Sync {
    /// Looks up a single value from the metastore.
    async fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>>;

    /// Looks
    async fn get_range(&self, start_key: &[u8], end_key: &[u8]) -> Result<Vec<KeyValueEntry>>;

    async fn get_prefix(&self, prefix: &[u8]) -> Result<Vec<KeyValueEntry>> {
        let (start_key, end_key) = prefix_key_range(prefix);
        self.get_range(&start_key, &end_key).await
    }

    async fn put(&self, key: &[u8], value: &[u8]) -> Result<()>;

    async fn delete(&self, key: &[u8]) -> Result<()>;

    async fn new_transaction<'a>(&'a self) -> Result<MetastoreTransaction<'a>>;
}

#[async_trait]
impl MetastoreClientInterface for MetastoreClient {
    async fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>> {
        self.get_impl(key, None).await
    }

    async fn get_range(&self, start_key: &[u8], end_key: &[u8]) -> Result<Vec<KeyValueEntry>> {
        self.get_range_impl(start_key, end_key, None).await
    }

    async fn put(&self, key: &[u8], value: &[u8]) -> Result<()> {
        self.put_impl(key, value).await
    }

    async fn delete(&self, key: &[u8]) -> Result<()> {
        self.delete_impl(key).await
    }

    async fn new_transaction<'a>(&'a self) -> Result<MetastoreTransaction<'a>> {
        self.new_transaction_impl().await
    }
}

pub struct MetastoreTransaction<'a> {
    class: MetastoreTransactionClass<'a>,
}

struct MetastoreTransactionState {
    read_index: u64,
    reads: Vec<KeyRange>,
    writes: BTreeMap<Bytes, Operation>,
}

enum MetastoreTransactionClass<'a> {
    TopLevel {
        client: &'a MetastoreClient,
        state: AsyncMutex<MetastoreTransactionState>,
    },
    /// A transaction that was started inside of another transaction. This is
    /// just a reference to the top level transaction.
    ///
    /// Committing a nested transaction is a no-op as it is instead committed
    /// later as part of the root transaction.
    Nested {
        client: &'a MetastoreClient,
        state: &'a AsyncMutex<MetastoreTransactionState>,
    },
}

#[async_trait]
impl<'a> MetastoreClientInterface for MetastoreTransaction<'a> {
    // TODO: Must also read from the local state
    async fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>> {
        self.get_impl(key).await
    }

    async fn get_range(&self, start_key: &[u8], end_key: &[u8]) -> Result<Vec<KeyValueEntry>> {
        self.get_range_impl(start_key, end_key).await
    }

    async fn put(&self, key: &[u8], value: &[u8]) -> Result<()> {
        self.put_impl(key, value).await
    }

    async fn delete(&self, key: &[u8]) -> Result<()> {
        self.delete_impl(key).await
    }

    async fn new_transaction<'b>(&'b self) -> Result<MetastoreTransaction<'b>> {
        self.new_transaction_impl().await
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
        &'b MetastoreClient,
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
    async fn get_impl(&self, key: &[u8]) -> Result<Option<Vec<u8>>> {
        let (client, state_permit) = self.get_top_level().await;

        let state = state_permit.read_exclusive();

        if let Some(op) = state.writes.get(key) {
            match op.typ_case() {
                OperationTypeCase::Put(value) => {
                    return Ok(Some(value.to_vec()));
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
    async fn get_range_impl(&self, start_key: &[u8], end_key: &[u8]) -> Result<Vec<KeyValueEntry>> {
        let (client, state_permit) = self.get_top_level().await;

        self.get_range_with_lock(start_key, end_key, client, state_permit)
            .await
    }

    /// CANCEL SAFE
    async fn get_range_with_lock(
        &self,
        start_key: &[u8],
        end_key: &[u8],
        client: &MetastoreClient,
        state_permit: AsyncMutexPermit<'_, MetastoreTransactionState>,
    ) -> Result<Vec<KeyValueEntry>> {
        let state = state_permit.read_exclusive();

        let written_values = {
            let mut out = vec![];
            for (_, op) in state
                .writes
                .range::<[u8], _>((Bound::Included(start_key), Bound::Excluded(end_key)))
            {
                let mut entry = KeyValueEntry::default();
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

//

/// TODO: This needs to detect retryable/cancellation related errors.
#[macro_export]
macro_rules! run_transaction {
    ($client:expr, $txn:ident, $f:expr) => {{
        let mut retval = None;
        for i in 0..$crate::MAX_TRANSACTION_RETRIES {
            let $txn = $client.new_transaction().await?;
            retval = Some($f);
            $txn.commit().await?;
            break;
        }

        retval.ok_or_else(|| err_msg("Transaction exceeded max number of retries"))?
    }};
}
