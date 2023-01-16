use std::sync::Arc;

use common::errors::*;

use crate::meta::client::ClusterMetaClient;
use crate::service::resolver::ServiceResolver;

pub async fn create_rpc_channel(
    address: &str,
    meta_client: Arc<ClusterMetaClient>,
) -> Result<Arc<dyn rpc::Channel>> {
    let resolver = Arc::new(ServiceResolver::create(address, meta_client).await?);

    Ok(Arc::new(rpc::Http2Channel::create(
        http::ClientOptions::from_resolver(resolver),
    )?))
}
