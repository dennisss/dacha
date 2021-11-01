use std::sync::Arc;

use common::async_std::sync::Mutex;
use common::{errors::*, task::ChildTask};
use sstable::table::KeyComparator;

use crate::proto::key_value::{Key, KeyRange, KeyValue, KeyValueStoreStub};

pub struct MetastoreClient {
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

        Ok(Self {
            route_store,
            channel,
            background_thread,
        })
    }

    pub async fn get(&self, key: &[u8]) -> Result<Vec<u8>> {
        let stub = KeyValueStoreStub::new(self.channel.clone());
        let request_context = rpc::ClientRequestContext::default();

        let mut request = Key::default();
        request.set_data(key);

        let response = stub.Get(&request_context, &request).await;

        Ok(response.result?.value().to_vec())
    }

    pub async fn put(&self, key: &[u8], value: &[u8]) -> Result<()> {
        let stub = KeyValueStoreStub::new(self.channel.clone());
        let request_context = rpc::ClientRequestContext::default();

        let mut key_value = KeyValue::default();
        key_value.set_key(key);
        key_value.set_value(value);

        stub.Put(&request_context, &key_value).await.result?;
        Ok(())
    }

    /// Lists all files in a directory (along with their contents.)
    pub async fn list(&self, dir: &str) -> Result<Vec<KeyValue>> {
        let stub = KeyValueStoreStub::new(self.channel.clone());
        let request_context = rpc::ClientRequestContext::default();

        let mut start_key = dir.to_string();
        if !start_key.ends_with("/") {
            start_key.push('/');
        }

        let end_key = sstable::table::BytewiseComparator::new()
            .find_short_successor(start_key.as_bytes().to_vec());

        let mut request = KeyRange::default();
        request.set_start_key(start_key.as_bytes());
        request.set_end_key(end_key);

        let mut out = vec![];

        let mut response = stub.GetRange(&request_context, &request).await;
        while let Some(kv) = response.recv().await {
            out.push(kv);
        }

        response.finish().await?;
        Ok(out)
    }

    pub async fn list_protos<M: protobuf::Message>(&self, dir: &str) -> Result<Vec<M>> {
        let mut out = vec![];
        for kv in self.list(dir).await? {
            out.push(M::parse(kv.value())?);
        }

        Ok(out)
    }
}
