#[macro_use]
extern crate macros;

use std::sync::Arc;

use cluster_client::id::entity_id_from_string;
use common::args::list::CommaSeparated;
use common::args::parse_args;
use common::errors::*;
use executor_multitask::RootResource;
use file::LocalPathBuf;
use rpc_util::NamedPortArg;

use cluster_client::meta::hostname::ClusterMetaHostnameResolver;
use datastore::meta::store::{run, MetastoreOptions};
use datastore::meta::EmbeddedDBStateMachineOptions;
use raft::{log::segmented_log::SegmentedLogOptions, proto::RouteLabel};

#[derive(Args)]
struct Args {
    port: NamedPortArg,
    dir: LocalPathBuf,
}

#[executor_main]
async fn main() -> Result<()> {
    let args = parse_args::<Args>()?;

    let creds = cluster_client::credentials::get_cluster_credentials().await?;

    let zone = std::env::var(cluster_client::env::ZONE_ENV_VAR)?;
    if zone.is_empty() {
        return Err(err_msg("Missing cluster zone in environment"));
    }

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

    let mut route_label = RouteLabel::default();
    route_label.set_value(format!("{}={}", cluster_client::env::ZONE_ENV_VAR, zone));

    let root = RootResource::new();

    root.register_dependency(creds.clone()).await;

    root.register_dependency(
        run(MetastoreOptions {
            dir: args.dir,
            init_port: None,
            bootstrap_group: false,
            bootstrap_node_id: Some(id),
            service_port: args.port.value(),
            route_labels: vec![route_label],
            log: SegmentedLogOptions::default(),
            state_machine: EmbeddedDBStateMachineOptions::default(),
            tls: Some(crypto::tls::Credentials {
                client: creds.client_options(),
                server: creds.server_options(),
            }),
            hostname_resolver: Arc::new(ClusterMetaHostnameResolver::new(&zone)),
        })
        .await?,
    )
    .await;

    root.wait().await
}
