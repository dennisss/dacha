use std::collections::{BTreeMap, HashMap};
use std::future::Future;
use std::ops::Bound;
use std::sync::Arc;

use common::async_fn::AsyncFn1;
use common::async_std::sync::{Mutex, MutexGuard};
use common::bytes::Bytes;
use common::{errors::*, task::ChildTask};
use sstable::table::KeyComparator;

use crate::meta::key_utils::*;
use crate::proto::client::*;
use crate::proto::key_value::*;

pub const MAX_TRANSACTION_RETRIES: usize = 5;

pub struct MetastoreClient {
    client_id: String,

    route_store: raft::RouteStore,

    channel: Arc<dyn rpc::Channel>,

    /// Background thread which maintains
    background_thread: ChildTask,
}

impl MetastoreClient {
    /// Creates a new client instance.
    ///
    /// The store servers will automatically be discovered.
    pub async fn create() -> Result<Self> {
        let route_store = raft::RouteStore::new();

        let discovery = raft::DiscoveryMulticast::create(route_store.clone()).await?;

        let background_thread = ChildTask::spawn(async move {
            eprintln!("DiscoveryClient exited: {:?}", discovery.run().await);
        });

        // TODO: When running in a cluster, we should use something like environment
        // variables from the node to ensure that this uses the right store.
        let channel_factory = raft::RouteChannelFactory::find_group(route_store.clone()).await;

        let channel = channel_factory.create_any()?;

        let client_id = {
            let stub = ClientManagementStub::new(channel.clone());

            let req = google::proto::empty::Empty::default();
            let ctx = rpc::ClientRequestContext::default();
            let res = stub.NewClient(&ctx, &req).await;

            res.result?.client_id().to_string()
        };

        Ok(Self {
            client_id,
            route_store,
            channel,
            background_thread,
        })
    }

    /// Request context to use if we are not running in a transaction.
    fn default_request_context(&self) -> Result<rpc::ClientRequestContext> {
        let mut request_context = rpc::ClientRequestContext::default();
        request_context
            .metadata
            .add_text(crate::meta::constants::CLIENT_ID_KEY, &self.client_id)?;
        Ok(request_context)
    }

    async fn get_impl(
        &self,
        key: &[u8],
        transaction_state: Option<&mut MetastoreTransactionState>,
    ) -> Result<Option<Vec<u8>>> {
        let stub = KeyValueStoreStub::new(self.channel.clone());
        let request_context = self.default_request_context()?;

        let mut request = ReadRequest::default();

        let (start_key, end_key) = single_key_range(key);
        request.keys_mut().set_start_key(start_key.as_ref());
        request.keys_mut().set_end_key(end_key.as_ref());

        if let Some(transaction_state) = transaction_state {
            request.set_read_index(transaction_state.read_index);
            transaction_state.reads.push(request.keys().clone());
        }

        let mut response = stub.Read(&request_context, &request).await;
        let value = if let Some(res) = response.recv().await {
            Some(res.entry().value().to_vec())
        } else {
            None
        };

        if value.is_some() {
            if response.recv().await.is_some() {
                return Err(err_msg("Received multiple values"));
            }
        }

        response.finish().await?;

        Ok(value)
    }

    /// Lists all files in a directory (along with their contents.)
    async fn get_range_impl(
        &self,
        start_key: &[u8],
        end_key: &[u8],
        transaction_state: Option<&mut MetastoreTransactionState>,
    ) -> Result<Vec<KeyValueEntry>> {
        let stub = KeyValueStoreStub::new(self.channel.clone());
        let request_context = self.default_request_context()?;

        let mut request = ReadRequest::default();

        request.keys_mut().set_start_key(start_key);
        request.keys_mut().set_end_key(end_key);

        // TODO: Deduplicate this code.
        if let Some(transaction_state) = transaction_state {
            request.set_read_index(transaction_state.read_index);
            transaction_state.reads.push(request.keys().clone());
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
        request.set_optimistic(true);

        let res = stub
            .Snapshot(&self.default_request_context()?, &request)
            .await
            .result?;

        Ok(MetastoreTransaction {
            class: MetastoreTransactionClass::TopLevel {
                client: self,
                state: Mutex::new(MetastoreTransactionState {
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
}

/// Interface for interacting with the metastore's key-value file system.
#[async_trait]
pub trait MetastoreClientInterface: Send + Sync {
    /// Looks up a single value from the metastore.
    async fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>>;

    async fn get_range(&self, start_key: &[u8], end_key: &[u8]) -> Result<Vec<KeyValueEntry>>;

    async fn get_prefix(&self, prefix: &[u8]) -> Result<Vec<KeyValueEntry>> {
        let (start_key, end_key) = prefix_key_range(prefix);
        self.get_range(&start_key, &end_key).await
    }

    /// Lists all key-value pairs in a directory.
    /// ('/' is used the path segmenter)
    async fn list(&self, dir: &[u8]) -> Result<Vec<KeyValueEntry>> {
        let (start_key, end_key) = directory_key_range(dir);
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
        state: Mutex<MetastoreTransactionState>,
    },
    /// A transaction that was started inside of another transaction. This is
    /// just
    Nested {
        client: &'a MetastoreClient,
        state: &'a Mutex<MetastoreTransactionState>,
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
        state.read_index
    }

    async fn get_top_level<'b>(
        &'b self,
    ) -> (
        &'b MetastoreClient,
        MutexGuard<'b, MetastoreTransactionState>,
    ) {
        match &self.class {
            MetastoreTransactionClass::TopLevel { client, state } => (*client, state.lock().await),
            MetastoreTransactionClass::Nested { client, state } => (*client, state.lock().await),
        }
    }

    async fn get_impl(&self, key: &[u8]) -> Result<Option<Vec<u8>>> {
        let (client, mut state) = self.get_top_level().await;

        if let Some(op) = state.writes.get(key) {
            match op.type_case() {
                OperationTypeCase::Put(value) => {
                    return Ok(Some(value.to_vec()));
                }
                OperationTypeCase::Delete(_) => {
                    return Ok(None);
                }
                OperationTypeCase::Unknown => {}
            }
        }

        client.get_impl(key, Some(&mut state)).await
    }

    async fn get_range_impl(&self, start_key: &[u8], end_key: &[u8]) -> Result<Vec<KeyValueEntry>> {
        let (client, mut state) = self.get_top_level().await;

        let written_values = {
            let mut out = vec![];
            for (_, op) in state
                .writes
                .range::<[u8], _>((Bound::Included(start_key), Bound::Excluded(end_key)))
            {
                let mut entry = KeyValueEntry::default();
                entry.set_key(op.key());

                match op.type_case() {
                    OperationTypeCase::Put(value) => {
                        entry.set_value(value.as_ref());
                    }
                    OperationTypeCase::Delete(_) | OperationTypeCase::Unknown => {
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
            .get_range_impl(start_key, end_key, Some(&mut state))
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
        let (_, mut state) = self.get_top_level().await;
        let mut op = Operation::default();
        op.set_key(key);
        op.set_put(value);
        state.writes.insert(key.into(), op);
        Ok(())
    }

    async fn delete_impl(&self, key: &[u8]) -> Result<()> {
        let (_, mut state) = self.get_top_level().await;
        let mut op = Operation::default();
        op.set_key(key);
        op.set_delete(true);
        state.writes.insert(key.into(), op);
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

        let (client, mut state) = self.get_top_level().await;

        if state.writes.is_empty() {
            return Ok(());
        }

        let stub = KeyValueStoreStub::new(client.channel.clone());
        let request_context = client.default_request_context()?;

        let mut request = ExecuteRequest::default();
        request.transaction_mut().set_read_index(state.read_index);

        for read in &state.reads {
            request.transaction_mut().add_reads(read.clone());
        }

        // NOTE: The keys should have already been added to each operation.
        for (_, op) in state.writes.iter() {
            request.transaction_mut().add_writes(op.clone());
        }

        stub.Execute(&request_context, &request).await.result?;
        Ok(())
    }
}

pub struct WatchStream {
    response: rpc::ClientStreamingResponse<KeyValueEntry>,
}

//

#[macro_export]
macro_rules! run_transaction {
    ($client:expr, $txn:ident, $f:expr) => {{
        let mut retval = None;
        for i in 0..$crate::meta::client::MAX_TRANSACTION_RETRIES {
            let $txn = $client.new_transaction().await?;
            retval = Some($f);
            $txn.commit().await?;
            break;
        }

        retval.ok_or_else(|| err_msg("Transaction exceeded max number of retries"))?
    }};
}