#[macro_use]
extern crate macros;

use std::sync::Arc;

use base_error::*;

use cluster_client::ClusterMetaClient;
use cluster_client::UserSessionAuthenticationIntoService;
use executor_multitask::RootResource;
use rpc_util::NamedPortArg;
use cluster_auth::*;

#[derive(Args)]
struct Args {
    port: NamedPortArg,
}

// All connections requests to this job will be authenticated since the peer entity will be the frontend job.
// But, before logging in, the effective entity will be unauthenticated.
const SERVICE_ACL_PROTO: &'static str = r#"
    allow_unauthenticated_connections: false

    allow_unauthenticated_web_assets: true

    rules: [
        {
            path: "/"
            is_directory: false
            principals: ["unauthenticated"]
        },
        # Performs its own ACLs checked internally.
        {
            path: "/rpc/cluster.UserSessionAuthentication"
            is_directory: true
            principals: ["unauthenticated"]
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

    let inst = Arc::new(ClusterAuthImpl::new(client.clone()).await);

    let mut server = cluster_client::ClusterServer::new(args.port.value(), acl, client)?;
    server.add_service(inst.clone().into_service())?;

    let web_handler = web::WebPageHandler::create(web::WebPageOptions {
        title: "Cluster Authentication".into(),
        script_path: "built/pkg/cluster/auth/app.js".into(),
        vars: None,
    }).await?;
    server.add_request_handler("/", false, web_handler)?;

    service.register_dependency(server.start()?).await;

    service.wait().await
}
