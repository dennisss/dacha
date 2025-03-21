// Cluster Management CLI
// TODO: Combine all of the CLI utilities into this one.
/*
Aside from the 'bootstrap' command, all commands require

TODO: Remove this list since it is mostly obsolete now.

Testing:
    cargo run --bin cluster_node -- --config=pkg/container/config/node.textproto

    cargo run --bin cluster -- start_worker --node_addr=127.0.0.1:10400 pkg/rpc_test/config/adder_server.worker

Next steps:
-

Testing with a single node cluster:
    cargo run --bin cluster_node -- --config=pkg/container/config/node.textproto --zone=dev

    cargo run --bin cluster -- bootstrap --node_addr=127.0.0.1:10400

    CLUSTER_ZONE=dev cargo run --bin cluster -- list jobs

    CLUSTER_ZONE=dev cargo run --bin cluster -- start_job pkg/rpc_test/config/adder_server.job

    CLUSTER_ZONE=dev cargo run --bin adder_client -- --target=adder_server.job.local.cluster.internal

    CLUSTER_ZONE=dev cargo run --bin cluster -- log --worker_name=adder_server.256326fbfc425883

    <try modifying the adder_server job and rerunning the start_job / adder_client code to verify that we can update to the new revision>

    <try stopping and restarting the node. everything should still work>



    cargo run --package rpc_util -- ls 10.1.1.1:30001 --insecure

    cargo run --bin cluster -- log --worker_name=system.manager.ftc5j9f006k0v

Testing with a single node non-cluster:
    cargo run --bin cluster_node -- --config=pkg/container/config/node.textproto

    cargo run --bin cluster -- start_worker pkg/rpc_test/config/adder_server.task --node_addr=127.0.0.1:10400

    cargo run --bin adder_client -- add 1 2 --target=127.0.0.1:30001

    cargo run --bin adder_client -- busy_loop 0.1 --target=127.0.0.1:30001

    cargo run --bin cluster -- list workers --node_addr=127.0.0.1:10400


Doing local node init:

- Create a root private key / cert
    - Load from a directory

- Locally start metastore
- [Custom] Build the node binary
- [Custom] Create a temp dir to store the node
- [Custom] Copy over the binary and config stuff to the node.
- .. Rest is the same

TODO: Allow deploying to a VM in order to test the remote setup_node via SSH.

TODO: Regard a bundle spec ion the node to define its current binary.



- Eventually need to re-up the CA certificates
- Eventually need to re-up the root certificate
    - Both of these operations will require manual intervention.
    - But do need a notification



ACL System:
- Typical Usecases:
    - Limit some key range in the metastore (readers/writers/owner)
        - May need nested key range ACLs
        - This will always be using an inheritance model
            - To add a new ACL, I look up all the existing ACLs and add a parent pointer
    - Limit execute access to specific RPC methods on the server
        - RPCs may have arguments that require further filtering
    - For a specific job prefix, allow job creation / mutation

- General definitions
    - Users : Things with identifies that can take actions / have permissions
        - Users have a 'type' and a name/id
            - Metastore entities are things with TLS certificates
    - Entites : e.g. a single file, a single job

----


- Create the CA job via the manager
    - Node will fail to create it since it can't contact an existing CA to get a cert for the worker
- Locally look up the worker in the metastore and push it to the node with a valid certificate
    - The CA will start running and will generate a new private key in a TPM
    - CA will put a CSR in the metastore



- Add the metastore worker to the node (directly talk to the node)
    - Will need to give it a key/certificate set to use
- Locally start a manager job
    - Will create the regular manager job and



TODO: Need to deprecate all node_addr usages since these don't contain a host name for doing TLS verification
*/

#[macro_use]
extern crate common;
#[macro_use]
extern crate macros;
#[macro_use]
extern crate file;

use std::convert::TryFrom;
use std::str::FromStr;
use std::time::Duration;

use cluster_client::credentials::get_cluster_credentials;
use cluster_client::env::ZONE_ENV_VAR;
use cluster_client::id::entity_id_to_string;
use cluster_client::meta::constants::META_STORE_SEEDS_ENV_VAR;
use common::errors::*;
use common::failure::ResultExt;
use common::io::{Readable, Writeable};

use cluster_cli::*;
use cluster_client::meta::client::ClusterMetaClient;
use cluster_client::meta::*;

#[derive(Args)]
pub struct Args {
    command: Command,
}

#[derive(Args)]
enum Command {
    // /// Initializes a new cluster. This should only be called once when
    // /// initially setting up a new set of nodes.
    // ///
    // /// Before this is run, there must already be at least one node machine
    // /// running.
    // #[arg(name = "bootstrap")]
    // Bootstrap(BootstrapCommand),
    #[arg(name = "setup_node")]
    SetupNode(SetupNodeCommand),

    /// Re-builds all system cluster components (metastore, manager) and updates
    /// them in a running cluster.
    ///
    /// TODO: Eventually also update node runtimes.
    ///
    /// TODO: Also renew the root and CA certificates if needed.
    #[arg(name = "upgrade")]
    Upgrade(UpgradeCommand),

    /// Enumerate objects in the cluster (workers, )
    #[arg(name = "list")]
    List(ListCommand),

    #[arg(name = "start_job")]
    StartJob(StartJobCommand),

    /// Start a single worker directly on a node. This is mainly for cluster
    /// bootstrapping.
    #[arg(name = "start_worker")]
    StartWorker(StartWorkerCommand),

    #[arg(name = "events")]
    Events(EventsCommand),

    /// Retrieve the log (stdout/stderr) outputs of a worker.
    #[arg(name = "log")]
    Log(LogCommand),

    #[arg(name = "labels")]
    Labels(LabelsCommand),

    #[arg(name = "envvars")]
    EnvVars(EnvVarsCommand),
}

#[derive(Args)]
struct EnvVarsCommand {}

async fn run_envvars(cmd: EnvVarsCommand) -> Result<()> {
    let meta_client = ClusterMetaClient::create_from_environment().await?;

    // Wait for server discovery.
    // TODO: Instead check that the RouteStore has marked initializers as done
    // running.
    executor::sleep(Duration::from_secs(4)).await;

    let seeds = meta_client.seeds().await?;

    let zone_var = format!("export {}={}", ZONE_ENV_VAR, meta_client.zone());
    let seed_var = format!("export {}={}", META_STORE_SEEDS_ENV_VAR, seeds);

    println!(
        "Append the following to ~/.bashrc:\n\n{}\n{}\n",
        zone_var, seed_var
    );

    Ok(())
}

#[executor_main]
async fn main() -> Result<()> {
    let args = common::args::parse_args::<Args>()?;
    match args.command {
        Command::SetupNode(cmd) => run_setup_node(cmd).await,
        Command::Upgrade(cmd) => run_upgrade(cmd).await,
        Command::List(cmd) => run_list(cmd).await,
        Command::StartWorker(cmd) => run_start_worker(cmd).await,
        Command::Log(cmd) => run_log(cmd).await,
        Command::StartJob(cmd) => run_start_job(cmd).await,
        Command::Events(cmd) => run_events(cmd).await,
        Command::Labels(cmd) => run_labels(cmd).await,
        Command::EnvVars(cmd) => run_envvars(cmd).await,
    }
}
