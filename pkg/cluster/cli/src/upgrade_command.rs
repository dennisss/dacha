use cluster_client::ClusterMetaClient;
use common::errors::*;

use crate::start_job_command::start_job_impl;
use crate::system_jobs::*;
use crate::utils::*;
use crate::acl::*;
use crate::bridge::*;

/*
TODO: Currently must be run with SUDO

```
CLUSTER_SUDO=yes cargo run --bin cluster_cli -- upgrade
```

*/

#[derive(Args)]
pub struct UpgradeCommand {}

pub async fn run_upgrade(cmd: UpgradeCommand) -> Result<()> {

    // // TODO: Also update the bridge if we have it setup (and re-apply any login scripts).
    // setup_bridge(true).await?;
    // return Ok(());

    let meta_client = ClusterMetaClient::create_from_environment().await?;
    let manager_stub = connect_to_manager(meta_client.clone()).await?;
    let request_context = rpc::ClientRequestContext::default();

    /*
    // Start a local manager instance.
    let manager =
        Manager::new(meta_client.clone(), Arc::new(crypto::random::global_rng())).into_service();
    let manager_channel = Arc::new(rpc::LocalChannel::new(manager));
    let manager_stub = cluster_client::ManagerStub::new(manager_channel);
    */

    upgrade_acls(meta_client.zone(), &meta_client.db(), false).await?;

    let mut specs = vec![
        get_ca_job().await?,
        get_metastore_job(meta_client.zone()).await?,
        get_manager_job().await?
    ];

    for spec in specs {
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
