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

const SERVICE_ACL_PROTO: &'static str = r#"

    allow_unauthenticated: false

    rules: [
        # Does its own ACL checks internally.
        {
            path: "/rpc/cluster.CertificateAuthority"
            is_directory: true
            principals: ["authenticated"]
        }
    ]
"#;

#[executor_main]
async fn main() -> Result<()> {
    let args = common::args::parse_args::<Args>()?;

    let creds = get_cluster_credentials().await?;
    let client = ClusterMetaClient::create_from_environment().await?;

    let mut acl = container_proto::cluster::ServiceACLProto::default();
    protobuf::text::parse_text_proto(SERVICE_ACL_PROTO, &mut acl)?;

    let service = RootResource::new();

    service.register_dependency(client.clone()).await;

    let inst = CertificateAuthorityImpl::create(client.clone()).await?;
    // service
    //     .spawn_interruptable("Manager::run", manager.clone().run())
    //     .await;

    let mut server = cluster_client::ClusterServer::new(args.port.value(), acl, client)?;
    server.add_service(inst.into_service())?;
    service.register_dependency(server.start()?).await;

    service.wait().await
}
