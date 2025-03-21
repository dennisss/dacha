#[macro_use]
extern crate macros;

use base_error::*;
use cluster_ca::CertificateAuthorityImpl;
use cluster_client::credentials::get_cluster_credentials;
use cluster_client::meta::client::ClusterMetaClient;
use cluster_client::CertificateAuthorityIntoService;
use executor_multitask::RootResource;
use rpc_util::{AddReflection, NamedPortArg};

#[derive(Args)]
struct Args {
    port: NamedPortArg,
}

#[executor_main]
async fn main() -> Result<()> {
    let args = common::args::parse_args::<Args>()?;

    let creds = get_cluster_credentials().await?;
    let client = ClusterMetaClient::create_from_environment().await?;

    let service = RootResource::new();

    service.register_dependency(client.clone()).await;

    let inst = CertificateAuthorityImpl::create(client.clone()).await?;
    // service
    //     .spawn_interruptable("Manager::run", manager.clone().run())
    //     .await;

    let mut server = rpc::Http2Server::new(Some(args.port.value()));
    server.http_options_mut().tls = Some(creds.server_options());
    server.set_base_path("/rpc"); // TODO: Standardize.
    server.add_service(inst.into_service())?;
    server.add_reflection()?;
    service.register_dependency(server.start()).await;

    service.wait().await
}
