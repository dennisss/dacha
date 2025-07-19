#[macro_use]
extern crate macros;

use std::sync::Arc;

use base_error::*;
use cluster_ca::CertificateAuthorityImpl;
use cluster_client::ClusterMetaClient;
use cluster_client::{CertificateAuthorityIntoService, UserAuthenticationIntoService};
use executor_multitask::RootResource;
use rpc_util::NamedPortArg;

#[derive(Args)]
struct Args {
    port: NamedPortArg,
}

const SERVICE_ACL_PROTO: &'static str = r#"
    # Allowed to enable user login requests.
    allow_unauthenticated_connections: true

    rules: [
        # Does its own ACL checks internally.
        {
            path: "/rpc/cluster.CertificateAuthority/SignCertificate"
            is_directory: false
            principals: ["authenticated"]
        },
        {
            path: "/rpc/cluster.CertificateAuthority/GetCertificateRegistry"
            is_directory: false
            principals: ["unauthenticated"]
        },
        {
            path: "/rpc/cluster.UserAuthentication/Login"
            is_directory: false
            principals: ["unauthenticated"]
        },
        {
            path: "/rpc/cluster.UserAuthentication/ChangePassword"
            is_directory: false
            principals: ["authenticated"]
        },
        {
            path: "/rpc/cluster.UserAuthentication/CreateUser"
            is_directory: false
            principals: ["group:cluster-owners"]
        }
    ]
"#;

#[executor_main]
async fn main() -> Result<()> {
    let args = common::args::parse_args::<Args>()?;

    let client = ClusterMetaClient::create_from_environment().await?;

    let mut acl = container_proto::cluster::ServiceACLProto::default();
    protobuf::text::parse_text_proto(SERVICE_ACL_PROTO, &mut acl)?;

    let service = RootResource::new();

    service.register_dependency(client.clone()).await;

    let inst = Arc::new(CertificateAuthorityImpl::create(client.clone()).await?);
    // service
    //     .spawn_interruptable("Manager::run", manager.clone().run())
    //     .await;

    let mut server = cluster_client::ClusterServer::new(args.port.value(), acl, client)?;
    server.add_service(CertificateAuthorityIntoService::into_service(inst.clone()))?;
    server.add_service(UserAuthenticationIntoService::into_service(inst.clone()))?;
    service.register_dependency(server.start()?).await;

    service.wait().await
}
