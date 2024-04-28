use std::sync::Arc;
use std::time::Duration;

use common::errors::*;
use crypto::random::{SharedRng, SharedRngExt};
use datastore_meta_client::{MetastoreClient, MetastoreClientInterface};
use executor::{cancellation::AlreadyCancelledToken, child_task::ChildTask};
use executor_multitask::ServiceResource;
use file::{temp::TempDir, LocalPathBuf};
use protobuf::text::ParseTextProto;
use raft::{log::segmented_log::SegmentedLogOptions, proto::RouteLabel};

use crate::{meta::EmbeddedDBStateMachineOptions, proto::KeyValueEntry};

/// In-process set of metastore instances for testing.
pub struct TestMetastoreCluster {
    shared: Arc<TestMetastoreClusterShared>,
}

struct TestMetastoreClusterShared {
    /// Root directory for all cluster data. Each node is given its own
    /// exclusive sub-directory.
    temp_dir: TempDir,
    route_labels: Vec<RouteLabel>,
}

impl TestMetastoreCluster {
    pub async fn create() -> Result<Self> {
        // TODO: Implement completely in memory.
        let temp_dir = TempDir::create()?;

        let route_labels = raft_client::utils::generate_unique_route_labels().await;

        Ok(Self {
            shared: Arc::new(TestMetastoreClusterShared {
                temp_dir,
                route_labels,
            }),
        })
    }

    pub async fn start_node(&self, index: usize, bootstrap: bool) -> Result<TestMetastore> {
        // TODO: Instead specifiy 0 and support retrieving the actual address from the
        // instance.
        let port = crypto::random::global_rng().between(8000, 10000).await;

        println!("< Node #{} running on port {} >", index, port);

        let dir = self.shared.temp_dir.path().join(index.to_string());

        let mut state_machine = EmbeddedDBStateMachineOptions::default();
        state_machine.db.write_buffer_size = 1 * 1024 * 1024;

        // TODO: Disable multicast as we don't need it in a unit test.
        let resource = crate::meta::store::run(crate::meta::store::MetastoreOptions {
            dir: dir.clone(),
            init_port: 0,
            bootstrap,
            service_port: port,
            route_labels: self.shared.route_labels.clone(),
            log: SegmentedLogOptions {
                target_segment_size: 1 * 1024 * 1024,
                max_segment_size: 2 * 1024 * 1024,
            },
            state_machine,
        })
        .await?;

        resource.wait_for_ready().await;

        // Wait for the leader election to finish.
        if bootstrap {
            executor::sleep(Duration::from_millis(400)).await?;
        }

        Ok(TestMetastore {
            cluster: self.shared.clone(),
            index,
            dir,
            port,
            resource,
        })
    }

    /// Creates a client which connects to all the nodes in this cluster.
    pub async fn create_client(&self) -> Result<MetastoreClient> {
        MetastoreClient::create(&self.shared.route_labels, &[]).await
    }
}

/// In-process single node metastore instance for testing.
pub struct TestMetastore {
    /// Reference to the cluster state to ensure that the cluster isn't deleted
    /// while the instance is live.
    cluster: Arc<TestMetastoreClusterShared>,

    index: usize,

    dir: LocalPathBuf,

    port: u16,

    resource: Arc<dyn ServiceResource>,
}

impl Drop for TestMetastore {
    fn drop(&mut self) {
        let resource = self.resource.clone();
        executor::spawn(async move {
            // TODO: Deduplicate with below.
            resource
                .add_cancellation_token(Arc::new(AlreadyCancelledToken::default()))
                .await;
            resource.wait_for_termination().await;
        });
    }
}

impl TestMetastore {
    pub async fn create() -> Result<Self> {
        let cluster = TestMetastoreCluster::create().await?;
        cluster.start_node(0, true).await
    }

    pub async fn close(self) -> Result<()> {
        println!("< Node #{} closing... >", self.index);

        self.resource
            .add_cancellation_token(Arc::new(AlreadyCancelledToken::default()))
            .await;
        self.resource.wait_for_termination().await
    }

    /// Creates a new client instance that is directly connected to just thie
    /// metastore node.
    pub async fn create_client(&self) -> Result<MetastoreClient> {
        MetastoreClient::create_direct(net::ip::SocketAddr::new(
            net::ip::IPAddress::V4([127, 0, 0, 1]),
            self.port,
        ))
        .await
    }

    pub async fn dir_contents(&self) -> Result<Vec<String>> {
        let mut out = vec![];

        file::recursively_list_dir(&self.dir, &mut |path| {
            out.push(path.strip_prefix(&self.dir).unwrap().to_string());
        })?;

        out.sort();

        Ok(out)
    }
}
