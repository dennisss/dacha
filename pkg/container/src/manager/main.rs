use std::sync::Arc;

use common::errors::*;
use executor::bundle::TaskResultBundle;
use rpc_util::{AddReflection, NamedPortArg};

use crate::manager::manager::Manager;
use crate::meta::client::ClusterMetaClient;
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

    let client = Arc::new(ClusterMetaClient::create_from_environment().await?);

    let mut bundle = TaskResultBundle::new();

    let manager = Manager::new(client, Arc::new(crypto::random::global_rng()));

    bundle.add("Manager::run()", manager.clone().run());

    let mut server = rpc::Http2Server::new();
    server.add_service(manager.into_service())?;
    server.add_reflection()?;
    server.set_shutdown_token(executor::signals::new_shutdown_token());
    bundle.add("Manager::serve", server.run(port));

    bundle.join().await?;

    Ok(())
}
