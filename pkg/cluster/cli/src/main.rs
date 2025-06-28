// Cluster Management CLI

#[macro_use]
extern crate common;
#[macro_use]
extern crate macros;
#[macro_use]
extern crate file;

use std::convert::TryFrom;
use std::str::FromStr;
use std::time::Duration;

use cluster_client::env::ZONE_ENV_VAR;
use cluster_client::id::entity_id_to_string;
use cluster_client::meta::constants::META_STORE_SEEDS_ENV_VAR;
use common::errors::*;
use common::failure::ResultExt;
use common::io::{Readable, Writeable};

use cluster_cli::*;
use cluster_client::ClusterMetaClient;
use cluster_client::meta::*;

#[derive(Args)]
pub struct Args {
    command: Command,
}

#[derive(Args)]
enum Command {
    /// Installs or upgrades the node runtime on a single machine via SSH.
    ///
    /// For the first node in a cluster, this should also be run with --bootstrap to
    /// optionally also bootstrap the entire cluster by installing core system jobs on the
    /// node.
    #[arg(name = "setup_node")]
    SetupNode(SetupNodeCommand),

    #[arg(name = "save_zone_config")]
    SaveZoneConfig(SaveZoneConfigCommand),

    #[arg(name = "load_zone_config")]
    LoadZoneConfig(LoadZoneConfigCommand),

    #[arg(name = "set_default_zone")]
    SetDefaultZone(SetDefaultZoneCommand),

    #[arg(name = "status")]
    Status(StatusCommand),

    #[arg(name = "create_user")]
    CreateUser(CreateUserCommand),

    #[arg(name = "login")]
    Login(LoginCommand),

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

    #[arg(name = "unlock")]
    Unlock(UnlockCommand),

    #[arg(name = "ping")]
    Ping(PingCommand),
}

#[executor_main]
async fn main() -> Result<()> {
    let args = common::args::parse_args::<Args>()?;
    match args.command {
        Command::SetupNode(cmd) => run_setup_node(cmd).await,
        Command::SaveZoneConfig(cmd) => run_save_zone_config(cmd).await,
        Command::LoadZoneConfig(cmd) => run_load_zone_config(cmd).await,
        Command::SetDefaultZone(cmd) => run_set_default_zone(cmd).await,
        Command::Status(cmd) => run_status(cmd).await,
        Command::CreateUser(cmd) => run_create_user(cmd).await,
        Command::Login(cmd) => run_login(cmd).await,
        Command::Upgrade(cmd) => run_upgrade(cmd).await,
        Command::List(cmd) => run_list(cmd).await,
        Command::StartWorker(cmd) => run_start_worker(cmd).await,
        Command::Log(cmd) => run_log(cmd).await,
        Command::StartJob(cmd) => run_start_job(cmd).await,
        Command::Events(cmd) => run_events(cmd).await,
        Command::Labels(cmd) => run_labels(cmd).await,
        Command::Unlock(cmd) => run_unlock(cmd).await,
        Command::Ping(cmd) => run_ping(cmd).await,
    }
}
