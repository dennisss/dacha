use std::sync::Arc;

use cluster_client::ClusterMetaClient;
use cluster_client::ClusterServer;
use common::errors::*;
use db_table::db::ProtobufDB;
use executor::bundle::TaskResultBundle;
use executor_multitask::RootResource;
use rpc_util::{AddReflection, NamedPortArg};
use cluster_proto::cluster::*;

use crate::Manager;

const SERVICE_ACL_PROTO: &'static str = r#"
    rules: [
        {
            path: "/rpc/cluster.Manager"
            is_directory: true
            principals: ["group:cluster-owners"]
        }
    ]
"#;

#[derive(Args)]
struct Args {
    port: NamedPortArg,
}

pub async fn main() -> Result<()> {
    let args = common::args::parse_args::<Args>()?;
    main_with_port(args.port.value()).await
}

async fn main_with_port(port: u16) -> Result<()> {
    // TODO: In order to shut down, the manager should release any locks it has.

    let service = RootResource::new();

    let client = ClusterMetaClient::create_from_environment().await?;
    service.register_dependency(client.clone()).await;

    let manager = Manager::new(
        client.zone(),
        client.db().clone(),
        Arc::new(crypto::random::global_rng()),
    );
    service
        .spawn_interruptable("Manager::run", manager.clone().run())
        .await;

    let mut acl = cluster_proto::cluster::ServiceACLProto::default();
    protobuf::text::parse_text_proto(SERVICE_ACL_PROTO, &mut acl)?;

    let mut server = ClusterServer::new(port, acl, client)?;
    server.add_service(manager.into_service())?;
    service.register_dependency(server.start()?).await;

    service.wait().await
}
