use std::sync::Arc;

use common::errors::*;

use crate::meta::client::ClusterMetaClient;
use crate::service::resolver::ServiceResolver;

pub async fn create_rpc_channel(
    address: &str,
    meta_client: Arc<ClusterMetaClient>,
) -> Result<Arc<dyn rpc::Channel>> {
    let resolver = Arc::new(ServiceResolver::create(address, meta_client.clone()).await?);

    let mut options: rpc::Http2ChannelOptions =
        http::ClientOptions::from_resolver(resolver).try_into_result()?;
    options.base_path = "/rpc".into();
    options.http.backend_balancer.backend.tls = meta_client.creds().map(|c| c.client);

    Ok(Arc::new(rpc::Http2Channel::create(options).await?))
}
