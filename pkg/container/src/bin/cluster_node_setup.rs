// This script helps to setup a new node machine that was just flashed if with a
// new linux environment.
//
// We will do the following:
// - Configure a unique number hostname of the form
//   'cluster-node-516fc8d828da45ba'
//   - This will use part of the machine id located at /etc/machine-id
// -

/*

cross build --target=armv7-unknown-linux-gnueabihf --bin cluster_node --release

/opt/dacha/bundle/...
/opt/dacha/data/cluster/...

*/

extern crate common;
#[macro_use]
extern crate macros;

use std::io::Write;
use std::path::Path;
use std::process::Command;

use common::async_std::task;
use common::{errors::*, project_dir};

#[derive(Args)]
struct Args {
    addr: String,
}

fn run_ssh(addr: &str, command: &str) -> Result<String> {
    let output = Command::new("ssh")
        .args([
            &format!("pi@{}", addr),
            "-i",
            "~/.ssh/id_cluster",
            "-o",
            "UserKnownHostsFile=/dev/null",
            "-o",
            "StrictHostKeyChecking=no",
            command,
        ])
        .output()?;
    if !output.status.success() {
        std::io::stdout().write_all(&output.stdout).unwrap();
        std::io::stderr().write_all(&output.stderr).unwrap();
        return Err(err_msg("Command failed"));
    }

    Ok(String::from_utf8(output.stdout)?)
}

fn copy_repo_file(addr: &str, relative_path: &str) -> Result<()> {
    println!("Copying //{}", relative_path);

    let repo_dir = "/opt/dacha/bundle";

    let source_path = project_dir().join(relative_path);

    let target_path = Path::new(repo_dir).join(relative_path);

    run_ssh(
        addr,
        &format!(
            "mkdir -p {}",
            target_path.parent().unwrap().to_str().unwrap()
        ),
    )?;

    {
        let output = Command::new("scp")
            .args([
                "-i",
                "~/.ssh/id_cluster",
                "-o",
                "UserKnownHostsFile=/dev/null",
                "-o",
                "StrictHostKeyChecking=no",
                source_path.to_str().unwrap(),
                &format!("pi@{}:{}", addr, target_path.to_str().unwrap()),
            ])
            .output()?;
        if !output.status.success() {
            std::io::stdout().write_all(&output.stdout).unwrap();
            std::io::stderr().write_all(&output.stderr).unwrap();
            return Err(err_msg("Command failed"));
        }
    }

    Ok(())
}

async fn run() -> Result<()> {
    let args = common::args::parse_args::<Args>()?;

    println!("Stopping old node");
    // This is currently a required step in order to be able to overwrite the in-use
    // files.
    //
    // NOTE: If the service doesn't exist yet, we'll ignore the error.
    run_ssh(&args.addr, "sudo systemctl stop cluster_node | true")?;

    let machine_id = run_ssh(&args.addr, "cat /etc/machine-id")?;
    let hostname = format!("cluster-node-{}", &machine_id[0..16]);

    println!("Setting hostname to: {}", hostname);
    run_ssh(
        &args.addr,
        &format!("sudo hostnamectl set-hostname {}", hostname),
    )?;

    run_ssh(&args.addr, "sudo mkdir -p /opt/dacha")?;
    run_ssh(&args.addr, "sudo chown pi:pi /opt/dacha")?;

    // Cluster cluster data directory
    run_ssh(&args.addr, "mkdir -p /opt/dacha/data")?;
    run_ssh(
        &args.addr,
        "sudo chown cluster-node:cluster-node /opt/dacha/data",
    )?;
    run_ssh(&args.addr, "sudo chmod 700 /opt/dacha/data")?;

    copy_repo_file(&args.addr, "built/pkg/container/cluster_node.armv7")?;
    copy_repo_file(&args.addr, "pkg/container/config/node.textproto")?;

    copy_repo_file(&args.addr, "pkg/container/config/cluster_node.service")?;

    // [target] [link_name]
    run_ssh(&args.addr, "sudo ln -f -s /opt/dacha/bundle/pkg/container/config/cluster_node.service /etc/systemd/system/cluster_node.service")?;

    run_ssh(&args.addr, "sudo systemctl enable cluster_node")?;
    run_ssh(&args.addr, "sudo systemctl start cluster_node")?;

    Ok(())
}

fn main() -> Result<()> {
    task::block_on(run())
}
