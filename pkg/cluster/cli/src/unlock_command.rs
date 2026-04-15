use std::collections::{HashSet, HashMap};

use cluster_client::meta::{client::ClusterMetaClient, NodeSchedulingMetadataTable};
use common::errors::*;
use cluster_proto::cluster::NodeSetupConfig;
use db_table::query_one;
use protobuf::Message;
use file::{LocalPathBuf, LocalPath};

use crate::ssh::*;

#[derive(Args)]
pub struct UnlockCommand {
    #[arg(positional)]
    config: LocalPathBuf,
}


pub async fn run_unlock(cmd: UnlockCommand) -> Result<()> {
    let config = {
        let data = file::read_to_string(&cmd.config).await?;
        let mut out = NodeSetupConfig::default();
        protobuf::text::parse_text_proto(&data, &mut out)?;
        out
    };

    let operator = SSHClient::new(config.node_ip(), config.node_user(), config.ssh_args().to_vec());
    let operator: &dyn MachineOperator = &operator;


    let mounted = get_already_mounted_datasets(operator).await?;

    // Map from remote path key to local path.
    let mut uploaded_keys = HashMap::new();

    for dataset in config.zfs_datasets() {
        if mounted.contains(dataset.name()) {
            println!("ZFS dataset '{}' already mounted.", dataset.name());
            continue;
        }

        let local_key_path = LocalPathBuf::from_trusted_string(dataset.local_key_path())?;
        let remote_key_path = LocalPath::new(dataset.remote_key_path()).normalized();
        
        // TODO: Need more uniform global checking that always using absolute paths for almost all operations.
        if !remote_key_path.is_absolute() {
            return Err(err_msg("Remote key path is not absolute"));
        }

        if let Some(old_local_path) = uploaded_keys.get(&remote_key_path) {
            if old_local_path != &local_key_path {
                return Err(err_msg("Uploading different keys to the same location"));
            }
        }

        let data = file::read(&local_key_path).await?;
        operator.upload_with(&data[..], &remote_key_path, UploadOptions::new().sudo()).await?;

        uploaded_keys.insert(remote_key_path, local_key_path);
    }

    if !uploaded_keys.is_empty() {
        println!("Loading all keys..");
        println!("{}", String::from_utf8(operator.run("sudo zfs load-key -a").await?)?);    
    }

    for dataset in config.zfs_datasets() {
        if mounted.contains(dataset.name()) {
            continue;
        }

        println!("Mount {}", dataset.name());
        operator.run(&format!("sudo zfs mount {}", dataset.name())).await?;
    }

    if !uploaded_keys.is_empty() {
        println!("Deleting keys..");
        println!("TODO");
        // TODO:
    }

    let mounted = get_already_mounted_datasets(operator).await?;
    for dataset in config.zfs_datasets() {
        if !mounted.contains(dataset.name()) {
            return Err(format_err!("Failed to mount ZFS dataset {}", dataset.name()));
        }
    }

    println!("Starting node service...");
    operator.run("sudo systemctl start cluster-node").await?;

    println!("Done!");

    Ok(())
}

async fn get_already_mounted_datasets(
    operator: &dyn MachineOperator
) -> Result<HashSet<String>> {
    let mounts = {
        let data = operator.download_string("/proc/mounts").await?;
        sys::Mount::parse_lines(&data)?
    };

    let mut out = HashSet::new();

    for mount in mounts {
        if mount.fs_type != "zfs" {
            continue;
        }

        out.insert(mount.device);
    }

    Ok(out)
}
