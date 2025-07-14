use std::sync::Arc;

use common::errors::*;

use http::{Resolver, StaticEndpointResolver, ResolvedEndpoint};
use net::ip::{SocketAddr, IPAddress};

use crate::meta::client::ClusterMetaClient;
use crate::service::resolver::ServiceResolver;

pub async fn create_rpc_channel(
    address: &str,
    meta_client: Arc<ClusterMetaClient>,
) -> Result<Arc<dyn rpc::Channel>> {
    let http_options = create_http_client_options(address, meta_client.clone())?;
    let mut options: rpc::Http2ChannelOptions = http_options.try_into_result()?;
    options.base_path = "/rpc".into();

    Ok(Arc::new(rpc::Http2Channel::create(options).await?))
}

pub async fn create_http_client(
    address: &str,
    meta_client: Arc<ClusterMetaClient>,
) -> Result<http::Client> {
    http::Client::create(create_http_client_options(address, meta_client)?).await
}

fn create_http_client_options(
    address: &str,
    meta_client: Arc<ClusterMetaClient>,
) -> Result<http::ClientOptions> {
    let resolver: Arc<dyn Resolver> = {
        if address.starts_with("localhost:") {
            let authority = address.parse::<http::uri::Authority>()?;
    
            Arc::new(StaticEndpointResolver::new(&[
                ResolvedEndpoint {
                    name: "".into(),
                    address: SocketAddr::new(IPAddress::V4([127,0,0,1]), authority.port.unwrap()),
                    authority: authority,
                }
            ]))    
        } else {
            Arc::new(ServiceResolver::create(address, meta_client.clone())?)
        }
    };

    let mut options = http::ClientOptions::from_resolver(resolver).set_force_http2(true);
    options.backend_balancer.backend.tls = meta_client.creds().map(|c| c.client);
    Ok(options)
}