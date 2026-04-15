/*
# Run remotely
sudo cp -R /zfs/flash/data/node/data/credentials/ ~/credentials_copy
sudo chmod -R o+rwx ~/credentials_copy/

# Run locally
scp -r nas:~/credentials_copy/ /tmp/credentials_copy

cargo run --bin cluster_cli -- refresh_node --zone=home --credentials_dir=/tmp/credentials_copy


scp -r /tmp/credentials_copy nas:~/credentials_copy_new


# Run remotely.
cd ~/credentials_copy_new

chmod 777 worker/system.meta.c8b1m3g8yneyj/certificate.pem
sudo cp worker/system.meta.c8b1m3g8yneyj/certificate.pem /zfs/flash/data/node/data/credentials/worker/system.meta.c8b1m3g8yneyj/certificate.pem

chmod 777 worker/system.cert-authority.kwbybrg7asfcd/certificate.pem

sudo cp worker/system.cert-authority.kwbybrg7asfcd/certificate.pem /zfs/flash/data/node/data/credentials/worker/system.cert-authority.kwbybrg7asfcd/certificate.pem

*/

use std::fmt::Debug;
use std::sync::Arc;
use std::time::{Instant, Duration};
use std::time::SystemTime;

use cluster_ca::*;
use cluster_client::id::{entity_id_to_string, normalize_entity_id};
use cluster_client::ClusterMetaClient;
use cluster_client::meta::*;
use cluster_client::service::create_rpc_channel;
use cluster_client::service::address::{ServiceAddress, ServiceEntity, ServiceName};
use builder::{BuildConfigTarget, Builder};
use common::errors::*;
use cluster_manager::Manager;
use container::NodeConfig;
use cluster_proto::cluster::*;
use crypto::random::{RngExt, SharedRngExt};
use crypto::tls::{Credentials, FileCredentialsManager};
use db_table::db::ProtobufDB;
use db_table::query_one;
use executor::cancellation::AlreadyCancelledToken;
use executor_multitask::ServiceResource;
use file::temp::TempDir;
use file::{project_dir, project_path, LocalPath, LocalPathBuf};
use hostname::{ClusterMetaHostnameResolver, ROOT_SERVER_ID};
use protobuf::text::{parse_text_proto, ParseTextProto};
use protobuf::Message;
use raft::log::segmented_log::SegmentedLogOptions;
use raft::proto::Configuration_ServerRole;

use crate::acl::{authorize_node, bootstrap_acls};
use crate::ssh::*;
use crate::start_job_command::start_job_impl;
use crate::system_jobs::*;
use crate::utils::*;
use crate::root_credentials::*;
use crate::create_user_command::{run_create_user_impl, read_stdin_password};
use crate::login_command::login_impl;

// TODO: Dedup
pub const NODE_CREDENTIALS_PATH: &'static str = "credentials/node";

// TODO: Dedup
pub const NODE_WORKER_CREDENTIALS_PATH: &'static str = "credentials/worker";

#[derive(Args)]
pub struct RefreshNodeCommand {
    // TODO: Grab this from the node or from the ambient env (and verify node is the same).
    zone: String,

    credentials_dir: LocalPathBuf
}

pub async fn run_refresh_node(cmd: RefreshNodeCommand) -> Result<()> {
    if !cluster_client::service::zone::is_valid_zone(&cmd.zone) {
        return Err(format_err!("Invalid --zone argument provided with value: {}", cmd.zone));
    }

    println!("Zone: {}", cmd.zone);

    let root_creds_dir = get_root_credentials_dir(&cmd.zone, &None)?;
    let root_creds =
        load_or_create_root_credentials(&root_creds_dir, &cmd.zone, false).await?;

    // TODO: Verify the node isn't currently running.

    {
        let node_credentials = FileCredentialsManager::create(&cmd.credentials_dir.join("node")).await?;

        let (certs, private_key) = node_credentials.certificates_with_private_key()
            .ok_or_else(|| err_msg("No existing node credentials"))?;
        assert_eq!(certs.len(), 1);

        let name = certs[0].subject().common_name()?.ok_or_else(|| err_msg("No common name"))?;

        println!("Node Common Name: {}", name);

        let now = SystemTime::now();
        // let not_before = SystemTime::from(certs[0].validity().not_before);
        let not_after = SystemTime::from(certs[0].validity().not_after);
        let time_remaining = not_after.duration_since(now).unwrap_or(Duration::ZERO);

        println!("Expires in {:?}", time_remaining);
    }

    let worker_credentials_dir = cmd.credentials_dir.join("worker");

    for entry in file::read_dir(&worker_credentials_dir)? {
        if !entry.name().starts_with("system.") {
            continue;
        }

        let path = worker_credentials_dir.join(entry.name());

        let mut credentials = FileCredentialsManager::create(&path).await?;

        let (old_certs, private_key) = credentials.certificates_with_private_key()
            .ok_or_else(|| err_msg("No existing node credentials"))?;
        assert_eq!(old_certs.len(), 1);

        let old_cname = old_certs[0].subject().common_name()?.ok_or_else(|| err_msg("No common name"))?;

        println!("Worker Common Name: {}", old_cname);

        let now = SystemTime::now();
        // let not_before = SystemTime::from(certs[0].validity().not_before);
        let not_after = SystemTime::from(old_certs[0].validity().not_after);
        let time_remaining = not_after.duration_since(now).unwrap_or(Duration::ZERO);

        println!("Expires in {:?}", time_remaining);

        if time_remaining > Duration::ZERO {
            continue;
        }

        println!("=> New Certificate");

        //

        let mut csr = crypto::x509::CertificateRequestBuilder::default();

        let name = ServiceName::for_worker(&cmd.zone, entry.name())?;
        let cname = name.to_string();
        assert_eq!(&cname, &old_cname);
        csr.set_common_name(&cname);

        let csr = csr.build(&private_key).await?;

        let cert =
            sign_leaf_certificate(&name, csr, &root_creds.certificate, &root_creds.private_key)
                .await?;

        credentials.write_certificates(&[cert.clone()], private_key.clone()).await?;
    }


    Ok(())
}