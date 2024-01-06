use common::errors::*;
use crypto::random::{SharedRng, SharedRngExt};
use datastore_meta_client::{MetastoreClient, MetastoreClientInterface};
use executor::child_task::ChildTask;
use file::temp::TempDir;
use protobuf::text::ParseTextProto;
use raft::proto::RouteLabel;

use crate::proto::KeyValueEntry;

/// In-process single node metastore instance for testing.
pub struct TestMetastore {
    temp_dir: TempDir,
    task: ChildTask,
    labels: Vec<RouteLabel>,
    port: u16,
}

impl TestMetastore {
    pub async fn create() -> Result<Self> {
        // TODO: Implement completely in memory.
        let temp_dir = TempDir::create()?;

        // TODO: Instead specifiy 0 and support retrieving the actual address from the
        // instance.
        let port = crypto::random::global_rng().between(8000, 10000).await;

        let route_labels = raft_client::utils::generate_unique_route_labels().await;

        // TODO: Disable multicast as we don't need it in a unit test.
        let fut = crate::meta::store::run(crate::meta::store::MetastoreConfig {
            dir: temp_dir.path().to_owned(),
            init_port: 0,
            bootstrap: true,
            service_port: port,
            route_labels: route_labels.clone(),
        });

        let task = ChildTask::spawn(async move { fut.await.unwrap() });

        Ok(Self {
            temp_dir,
            task,
            labels: route_labels,
            port,
        })
    }

    pub async fn create_client(&self) -> Result<MetastoreClient> {
        MetastoreClient::create_direct(net::ip::SocketAddr::new(
            net::ip::IPAddress::V4([127, 0, 0, 1]),
            self.port,
        ))
        .await

        // MetastoreClient::create(&self.labels).await
    }
}
