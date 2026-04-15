// Server that interacts with a USB NRF52 dongle for communicating with remote
// NRF52 devices via this server's exposed RPC interface.

#[macro_use]
extern crate common;
#[macro_use]
extern crate macros;

use std::sync::Arc;

use common::errors::*;
use executor_multitask::RootResource;
use cluster_client::{ClusterServer, ClusterMetaClient};
use nordic_tools_proto::nordic::*;


const SERVICE_ACL_PROTO: &'static str = r#"
    rules: [
        {
            path: "/rpc/nordic.RadioBridge"
            is_directory: true
            principals: ["group:cluster-owners"]
        }
    ]
"#;

#[derive(Args)]
struct Args {
    /// Name of the object in the metastore to be used for storing the state of
    /// this bridge.
    state_object_name: String,

    port: rpc_util::NamedPortArg,
}

#[executor_main]
async fn main() -> Result<()> {
    let args = common::args::parse_args::<Args>()?;

    let service = RootResource::new();

    let client = ClusterMetaClient::create_from_environment().await?;
    service.register_dependency(client.clone()).await;

    let mut acl = cluster_proto::cluster::ServiceACLProto::default();
    protobuf::text::parse_text_proto(SERVICE_ACL_PROTO, &mut acl)?;

    let mut server = ClusterServer::new(args.port.value(), acl, client.clone())?;

    let bridge =
        Arc::new(nordic_tools::radio_bridge::RadioBridge::create(
            client.clone(),
            &args.state_object_name,
        ).await?);
    service.register_dependency(bridge.clone()).await;
    server.add_service(bridge.clone().into_service())?;


    service.register_dependency(server.start()?).await;

    service.wait().await
}
