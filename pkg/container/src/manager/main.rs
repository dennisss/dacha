use std::sync::Arc;

use cluster_client::meta::client::ClusterMetaClient;
use common::errors::*;
use executor::bundle::TaskResultBundle;
use executor_multitask::RootResource;
use rpc_util::{AddReflection, NamedPortArg};

use crate::manager::manager::Manager;
use crate::proto::*;

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

    let client = Arc::new(ClusterMetaClient::create_from_environment().await?);
    service.register_dependency(client.clone()).await;

    let manager = Manager::new(client, Arc::new(crypto::random::global_rng()));
    service
        .spawn_interruptable("Manager::run", manager.clone().run())
        .await;

    let mut server = rpc::Http2Server::new(Some(port));
    server.add_service(manager.into_service())?;
    server.add_reflection()?;
    service.register_dependency(server.start()).await;

    service.wait().await
}
