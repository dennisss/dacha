use cluster_client::ClusterMetaClient;
use common::errors::*;
use common::args::list::CommaSeparated;

use crate::start_job_command::start_job_impl;
use crate::system_jobs::*;
use crate::utils::*;
use crate::acl::*;
use crate::bridge::*;

/*
TODO: Currently must be run with SUDO

```
CLUSTER_SUDO=yes cargo run --bin cluster_cli -- upgrade --components=ACL
```

*/

#[derive(Args)]
pub struct UpgradeCommand {
    #[arg(default = false)]
    local_manager: bool,
    components: CommaSeparated<Component>
}

#[derive(Args, PartialEq, Eq)]
pub enum Component {
    ALL,
    ACL,
    CA,
    METASTORE,
    MANAGER
}

impl UpgradeCommand {
    fn should_update(&self, component: Component) -> bool {
        self.components.values.contains(&component) ||
        self.components.values.contains(&Component::ALL)
    }
}

pub async fn run_upgrade(cmd: UpgradeCommand) -> Result<()> {

    println!("Generating Diff (not changing anything yet):");

    run_upgrade_inner(&cmd, false).await?;

    println!("");
    println!("Continue: [y/N]?");
    if !file::read_user_confirmation().await? {
        println!("[Exit without changing anything]");
        return Ok(());
    }

    // TODO: Ensure this does exactly what was listed in the diff.
    run_upgrade_inner(&cmd, true).await?;

    Ok(())
}

async fn run_upgrade_inner(cmd: &UpgradeCommand, write: bool) -> Result<()> {
    let meta_client = ClusterMetaClient::create_from_environment().await?;
    let manager_stub = connect_to_manager(meta_client.clone()).await?;
    let request_context = rpc::ClientRequestContext::default();

    println!("Zone: {}", meta_client.zone());

    if cmd.should_update(Component::ACL) {
        println!("# ACL Update");
        upgrade_acls(meta_client.zone(), &meta_client.db(), write).await?;
    }

    // // TODO: Also update the bridge if we have it setup (and re-apply any login scripts).
    // setup_bridge(true).await?;
    // return Ok(());



    /*
    // Start a local manager instance.
    let manager =
        Manager::new(meta_client.clone(), Arc::new(crypto::random::global_rng())).into_service();
    let manager_channel = Arc::new(rpc::LocalChannel::new(manager));
    let manager_stub = cluster_client::ManagerStub::new(manager_channel);
    */


    let mut specs = vec![];

    if cmd.should_update(Component::CA) {
        specs.push(get_ca_job().await?);
    }
    if cmd.should_update(Component::METASTORE) {
        specs.push(get_metastore_job(meta_client.zone()).await?);
    }
    if cmd.should_update(Component::MANAGER) {
        specs.push(get_manager_job().await?);
    }

    for spec in specs {
        println!("=> Build and push job: {}", spec.name());

        if !write {
            continue;
        }
        
        start_job_impl(
            meta_client.clone(),
            &manager_stub,
            &spec,
            &request_context,
        )
        .await?;

        // TODO: Wait until healthy.
    }

    Ok(())
}
