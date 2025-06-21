#[macro_use]
extern crate macros;

use std::sync::Arc;

use base_error::*;
use executor_multitask::RootResource;
use cluster_client::ClusterMetaClient;

/*
Testing

cargo build --package cluster_bridge
sudo setcap CAP_NET_BIND_SERVICE=+eip target/debug/cluster_bridge
target/debug/cluster_bridge

dig @127.0.0.80 google.com
dig @127.0.0.80 h818t68wbkmek.rpi_controller.worker.home.cluster.internal

resolvectl status
resolvectl query h818t68wbkmek.rpi_controller.worker.home.cluster.internal

curl -v https://h818t68wbkmek.rpi_controller.worker.home.cluster.internal/

curl -v http://h818t68wbkmek.rpi_controller.worker.home.cluster.internal/

*/

#[executor_main]
async fn main() -> Result<()> {
    let service = RootResource::new();

    let client = ClusterMetaClient::create_from_environment().await?;
    service.register_dependency(client.clone()).await;

    let dns_server = cluster_bridge::BridgeDNSServer::create(client.clone()).await?;
    service.register_dependency(Arc::new(dns_server)).await;

    let tls_server = cluster_bridge::BridgeTLSServer::create(client.clone()).await?;
    service.register_dependency(Arc::new(tls_server)).await;

    service.register_dependency(Arc::new(cluster_bridge::start_bridge_http_server())).await;

    service.wait().await
}
