use cluster_client::ClusterMetaClient;
use common::errors::*;

use crate::start_job_command::start_job_impl;
use crate::system_jobs::*;
use crate::utils::*;

#[derive(Args)]
pub struct UpgradeCommand {}

pub async fn run_upgrade(cmd: UpgradeCommand) -> Result<()> {
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

    // TODO: get_ca_job

    let meta_job_spec = get_metastore_job(meta_client.zone()).await?;
    start_job_impl(
        meta_client.clone(),
        &manager_stub,
        &meta_job_spec,
        &request_context,
    )
    .await?;

    let manager_job_spec = get_manager_job().await?;
    start_job_impl(
        meta_client.clone(),
        &manager_stub,
        &manager_job_spec,
        &request_context,
    )
    .await?;

    Ok(())
}
