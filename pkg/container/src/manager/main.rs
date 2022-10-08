use std::sync::Arc;

use common::bundle::TaskResultBundle;
use common::errors::*;
use rpc_util::{AddReflection, NamedPortArg};

use crate::manager::manager::Manager;
use crate::meta::client::ClusterMetaClient;
use crate::proto::manager::*;
use crate::proto::worker::*;

#[derive(Args)]
struct Args {
    port: NamedPortArg,
}

pub fn main() -> Result<()> {
    let args = common::args::parse_args::<Args>()?;
    common::async_std::task::block_on(main_with_port(args.port.value()))
}

async fn main_with_port(port: u16) -> Result<()> {
    // TODO: In order to shut down, the manager should release any locks it has.

    let client = Arc::new(ClusterMetaClient::create_from_environment().await?);

    let mut bundle = TaskResultBundle::new();

    let manager = Manager::new(client);

    bundle.add("Manager::run()", manager.clone().run());

    let mut server = rpc::Http2Server::new();
    server.add_service(manager.into_service())?;
    server.add_reflection()?;
    server.set_shutdown_token(common::shutdown::new_shutdown_token());
    bundle.add("Manager::serve", server.run(port));

    bundle.join().await?;

    Ok(())
}
