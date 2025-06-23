#[macro_use]
extern crate macros;

use std::sync::Arc;

use cluster_client::id::entity_id_from_string;
use cluster_client::{ClusterServer, ClusterMetaClient};
use cluster_meta::*;
use common::args::list::CommaSeparated;
use common::args::parse_args;
use common::errors::*;
use executor_multitask::RootResource;
use file::LocalPathBuf;
use rpc_util::NamedPortArg;

#[derive(Args)]
struct Args {
    port: NamedPortArg,
    dir: LocalPathBuf,
}

#[executor_main]
async fn main() -> Result<()> {
    let args = parse_args::<Args>()?;

    // TODO: We should be able to make this lighter and re-use more state from the main server.
    let client = ClusterMetaClient::create_from_environment().await?;

    let id = {
        // TODO: This is pretty easy to unit test.

        let my_name = std::env::var(cluster_client::env::WORKER_NAME_ENV_VAR)?;
        if my_name.is_empty() {
            return Err(err_msg("CA must be running as a cluster worker"));
        }

        let my_id = my_name
            .split('.')
            .last()
            .ok_or_else(|| err_msg("Can't find the worker id"))?;

        entity_id_from_string(&my_id).ok_or_else(|| err_msg("Invalid worker id"))?
    };

    let root = RootResource::new();

    // TODO: If this has issues finding the leader, then we should still enable the server to run.
    root.register_dependency(client.clone()).await;

    root.register_dependency(
        run(ClusterMetastoreOptions {
            id,
            port: args.port.value(),
            client,
            dir: args.dir,
            bootstrap: false,
        })
        .await?,
    )
    .await;

    root.wait().await
}
