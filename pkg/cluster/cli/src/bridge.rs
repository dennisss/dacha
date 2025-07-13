use common::errors::*;
use file::LocalPath;
use builder::Builder;
use file::project_path;

use crate::ssh::*;

const RESOLVED_CONFIG: &'static str = "
[Resolve]
DNS=127.0.0.80
";

// By default this won't re-setup things if it is already setup sufficiently.
pub async fn setup_bridge(force_binary_update: bool) -> Result<()> {
    let op = LocalOperator::default();
    let op: &dyn MachineOperator = &op;

    let home = std::env::var("HOME")?;
    let username = std::env::var("USER")?;

    // Must stop any existing service to allow nicely updating the file.
    // TODO: Do a smarter file replace in upload_file to avoid needing to do this.
    op.run("systemctl --user stop cluster-bridge | true").await?;

    let mut bin_updated = false;
    {
        println!("Building bridge binary...");

        let bin_path = LocalPath::new(&home).join(".dacha/services/cluster_bridge");
        if op.file_exists(&bin_path).await? && !force_binary_update {
            println!("=> Skipping since already present.")
        } else {
            op.create_dir_all(bin_path.parent().unwrap()).await?;

            // TODO: Need a more hermetic building environment that can't break if other processes are running build at the same time.
            let build_result = {
                let mut builder = Builder::default()?;
        
                let result = builder
                    .build_target_cwd("//pkg/cluster/bridge:cluster_bridge", "//pkg/builder/config:x64")
                    .await?;
        
                result
            };

            if build_result.outputs.output_files.len() != 1 {
                return Err(err_msg("Expected exactly one binary to be built"));
            }

            let file = build_result.outputs.output_files.values().next().unwrap();
            op.upload_file(file.location.clone(), &bin_path).await?;

            bin_updated = true;
        }

        // TODO: Also consistently check the file owner all the time.
        op.run(&format!("chmod 700 {}", bin_path.as_str())).await?;

        let caps = String::from_utf8(op.run(&format!("getcap {}", bin_path.as_str())).await?)?;
        if !caps.contains("cap_net_bind_service=eip") {
            println!("Setting binary capabilities..");
            op.run(&format!("sudo setcap CAP_NET_BIND_SERVICE=+eip {}", bin_path.as_str())).await?;
            println!("=> Done");
            bin_updated = true;
        }
    }

    {
        println!("Setting up bridge systemd service...");

        let service_path = LocalPath::new(&home).join(".config/systemd/user/cluster-bridge.service");

        let service = file::read_to_string(project_path!("pkg/cluster/config/bridge.service")).await?
            .replace("{HOME}", &home);

        op.create_dir_all(service_path.parent().unwrap()).await?;
        op.upload(service.as_bytes(), &service_path).await?;

        op.run("systemctl --user enable cluster-bridge").await?;
        op.run("systemctl --user start cluster-bridge").await?;

        println!("=> Done");
    }

    {
        println!("Setting local system DNS settings...");

        if !op.file_exists("/etc/systemd/resolved.conf").await? {
            return Err(err_msg("Local system is not using systemd-resolved"));
        }
   
        if !op.file_exists("/etc/systemd/resolved.conf.d").await? {
            op.run("sudo mkdir /etc/systemd/resolved.conf.d").await?;
        }

        let config = RESOLVED_CONFIG.trim();
        let config_path = "/etc/systemd/resolved.conf.d/dacha-cluster.conf";

        if op.file_exists(&config_path).await? && op.download_string(&config_path).await? == config {
            // TODO: It is possible that it the file was created but the resolved service hasn't reloaded it.
            println!("=> Already set up");
        } else {
            op.upload_with(config.as_bytes(), config_path, &UploadOptions::new().sudo()).await?;
            op.run("sudo systemctl restart systemd-resolved").await?;
            println!("=> Done");
        }
    }

    Ok(())
}