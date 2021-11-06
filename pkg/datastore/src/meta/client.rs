use std::sync::Arc;

use common::async_std::sync::Mutex;
use common::{errors::*, task::ChildTask};
use sstable::table::KeyComparator;

use crate::proto::client::*;
use crate::proto::key_value::{Key, KeyRange, KeyValue, KeyValueStoreStub, WatchRequest};

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

    fn get_request_context(&self) -> Result<rpc::ClientRequestContext> {
        let mut request_context = rpc::ClientRequestContext::default();
        request_context
            .metadata
            .add_text(crate::meta::constants::CLIENT_ID_KEY, &self.client_id)?;
        Ok(request_context)
    }

    pub async fn get(&self, key: &[u8]) -> Result<Vec<u8>> {
        let stub = KeyValueStoreStub::new(self.channel.clone());
        let request_context = self.get_request_context()?;

        let mut request = Key::default();
        request.set_data(key);

        let response = stub.Get(&request_context, &request).await;

        Ok(response.result?.value().to_vec())
    }

    pub async fn get_proto<M: protobuf::Message>(&self, key: &[u8]) -> Result<M> {
        let data = self.get(key).await?;
        M::parse(&data)
    }

    pub async fn put(&self, key: &[u8], value: &[u8]) -> Result<()> {
        let stub = KeyValueStoreStub::new(self.channel.clone());
        let request_context = self.get_request_context()?;

        let mut key_value = KeyValue::default();
        key_value.set_key(key);
        key_value.set_value(value);

        stub.Put(&request_context, &key_value).await.result?;
        Ok(())
    }

    /// Lists all files in a directory (along with their contents.)
    pub async fn list(&self, dir: &str) -> Result<Vec<KeyValue>> {
        let stub = KeyValueStoreStub::new(self.channel.clone());
        let request_context = self.get_request_context()?;

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

    /// NOTE: Once this returns, all future changess creates by any other client
    /// will be acounted for.
    pub async fn watch(&self, key_prefix: &str) -> Result<WatchStream> {
        let stub = KeyValueStoreStub::new(self.channel.clone());
        let request_context = self.get_request_context()?;

        let mut request = WatchRequest::default();
        request.set_key_prefix(key_prefix.as_bytes());

        let mut response = stub.Watch(&request_context, &request).await;

        // TODO:
        response.recv_head().await;

        Ok(WatchStream { response })
    }
}

pub struct WatchStream {
    response: rpc::ClientStreamingResponse<KeyValue>,
}
