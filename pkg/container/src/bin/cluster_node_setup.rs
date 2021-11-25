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
extern crate container;

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

fn run_scp(source: &str, destination: &str) -> Result<()> {
    let output = Command::new("scp")
        .args([
            "-i",
            "~/.ssh/id_cluster",
            "-o",
            "UserKnownHostsFile=/dev/null",
            "-o",
            "StrictHostKeyChecking=no",
            source,
            destination,
        ])
        .output()?;
    if !output.status.success() {
        std::io::stdout().write_all(&output.stdout).unwrap();
        std::io::stderr().write_all(&output.stderr).unwrap();
        return Err(err_msg("Command failed"));
    }

    Ok(())
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

    run_scp(
        source_path.to_str().unwrap(),
        &format!("pi@{}:{}", addr, target_path.to_str().unwrap()),
    )?;

    Ok(())
}

fn download_file(addr: &str, path: &str, output_path: &str) -> Result<()> {
    run_scp(&format!("pi@{}:{}", addr, path), output_path)
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

    // TODO: Need to re-build this (and use a platform independent name).
    copy_repo_file(&args.addr, "built/pkg/container/cluster_node.armv7")?;
    copy_repo_file(&args.addr, "pkg/container/config/node.textproto")?;

    copy_repo_file(&args.addr, "pkg/container/config/node.service")?;

    // [target] [link_name]
    run_ssh(&args.addr, "sudo ln -f -s /opt/dacha/bundle/pkg/container/config/node.service /etc/systemd/system/cluster_node.service")?;

    println!("Setting up /etc/subgid");
    {
        let tmpdir = common::temp::TempDir::create()?;
        let groups_path = tmpdir.path().join("group");

        download_file(&args.addr, "/etc/group", groups_path.to_str().unwrap())?;

        let groups = container::node::shadow::read_groups_from_path(groups_path)?;

        let mut subgid = String::new();
        subgid.push_str("cluster-node:400000:65536\n");

        let target_groups = &["gpio", "plugdev", "dialout", "i2c", "spi", "video"];

        for group in groups {
            if target_groups.iter().find(|g| *g == &group.name).is_some() {
                subgid.push_str(&format!("cluster-node:{}:1\n", group.id));
            }
        }

        println!("{}", subgid);
        println!("");

        let subgid_path = tmpdir.path().join("subgid");
        std::fs::write(&subgid_path, subgid)?;

        run_scp(
            &subgid_path.to_str().unwrap(),
            &format!("pi@{}:/tmp/next_subgid", &args.addr),
        )?;

        run_ssh(&args.addr, "sudo cp /tmp/next_subgid /etc/subgid")?;
    }

    // TODO: Also keep other files like /boot/config.txt in sync

    run_ssh(&args.addr, "sudo systemctl enable cluster_node")?;
    run_ssh(&args.addr, "sudo systemctl start cluster_node")?;

    Ok(())
}

fn main() -> Result<()> {
    task::block_on(run())
}
