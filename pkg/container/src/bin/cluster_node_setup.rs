// This script helps to setup a new node machine that was just flashed if with a
// new linux environment.
//
// We will do the following:
// - Configure a unique number hostname of the form 'cluster-node-e77h92tfgzdvf'
//   - This will use part of the machine id located at /etc/machine-id
// -

/*
/opt/dacha/bundle/...
/opt/dacha/data/cluster/...

Safety mesaures needed:
- Must have a well defined local system time before the node can start running.
- Need automatic detection on each RPC of clock syncronization

*/

#[macro_use]
extern crate common;
#[macro_use]
extern crate macros;
extern crate container;
#[macro_use]
extern crate regexp_macros;
extern crate automata;
extern crate builder;
#[macro_use]
extern crate file;

use std::fmt::Debug;
use std::io::Write;
use std::process::Command;

use builder::{BuildConfigTarget, Builder};
use common::errors::*;
use file::{project_dir, LocalPath};
use protobuf::text::{parse_text_proto, ParseTextProto};

use container::NodeConfig;

/// Name of the user on the node's Linux system which will own all main files
/// like node binaries.
const ASSET_OWNER: &'static str = "cluster-user";

/// Name of the user on the node's Linux system which will execute the node
/// binary.
const NODE_USER: &'static str = "cluster-node";

// TODO: Support parsing "\\n" in a regexp?
// TODO: Support specifying that the pattern must start at the beginning of the
// line
// TODO: Make case insensitive.
regexp!(LSCPU_ARCHITECTURE => "(?:^|\n)Architecture:\\s+([^\n]+)\n");
regexp!(CPUINFO_MODEL => "(?:^|\n)Model\\s+:\\s+([^\n]+)\n");

#[derive(Args)]
struct Args {
    addr: String,

    zone: String,
}

fn run_ssh(addr: &str, command: &str) -> Result<String> {
    let output = Command::new("ssh")
        .args([
            &format!("{}@{}", ASSET_OWNER, addr),
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

fn copy_file<P: AsRef<LocalPath> + Debug, Q: AsRef<LocalPath> + Debug>(
    addr: &str,
    source_path: P,
    target_repo_path: Q,
) -> Result<()> {
    let repo_dir = "/opt/dacha/bundle";
    let target_path = LocalPath::new(repo_dir).join(target_repo_path.as_ref());

    println!("{:?} => {:?}", source_path, target_path);

    run_ssh(
        addr,
        &format!("mkdir -p {}", target_path.parent().unwrap().as_str()),
    )?;

    run_scp(
        source_path.as_ref().as_str(),
        &format!("{}@{}:{}", ASSET_OWNER, addr, target_path.as_str()),
    )?;

    Ok(())
}

fn copy_repo_file<P: AsRef<LocalPath> + Debug>(addr: &str, relative_path: P) -> Result<()> {
    let source_path = {
        let p = relative_path.as_ref();
        if p.is_absolute() {
            p.to_owned()
        } else {
            project_dir().join(p)
        }
    };

    copy_file(addr, source_path, relative_path)
}

fn download_file(addr: &str, path: &str, output_path: &str) -> Result<()> {
    run_scp(&format!("{}@{}:{}", ASSET_OWNER, addr, path), output_path)
}

#[executor_main]
async fn main() -> Result<()> {
    let args = common::args::parse_args::<Args>()?;

    println!(
        "Bootstrapping node at \"{}\" in zone \"{}\"",
        args.addr, args.zone
    );

    if args.zone.is_empty() {
        return Err(err_msg("Empty --zone provided"));
    }

    println!("Stopping old node");
    // This is currently a required step in order to be able to overwrite the in-use
    // files.
    //
    // NOTE: If the service doesn't exist yet, we'll ignore the error.
    run_ssh(&args.addr, "sudo systemctl stop cluster-node | true")?;

    // TODO: Randomize this here just incase the machine-id wasn't regenerated since
    // imaging the target machine.
    let machine_id = {
        let hex = run_ssh(&args.addr, "cat /etc/machine-id")?;
        let data = base_radix::hex_decode(hex.trim())?;
        u64::from_be_bytes(*array_ref![data, 0, 8])
    };
    let hostname = format!(
        "cluster-node-{}",
        base_radix::base32_encode_cl64(machine_id)
    );

    let build_config_target = {
        let lscpu_output = run_ssh(&args.addr, "lscpu")?;
        let cpuinfo_output = run_ssh(&args.addr, "cat /proc/cpuinfo")?;

        let architecture = LSCPU_ARCHITECTURE
            .exec(&lscpu_output)
            .unwrap()
            .group_str(1)
            .unwrap()?
            .to_string();
        println!("Architecture: {}", architecture);

        let model = CPUINFO_MODEL
            .exec(&cpuinfo_output)
            .unwrap()
            .group_str(1)
            .unwrap()?
            .to_string();

        if architecture == "aarch64" && model.contains("Raspberry Pi") {
            "//pkg/builder/config:rpi64"
        } else if architecture == "x86_64" {
            "//pkg/builder/config:x64"
        } else {
            return Err(format_err!(
                "Unsupported CPU type: {} | {}",
                architecture,
                model
            ));
        }
    };

    println!("Building node runtime with {}", build_config_target);

    let node_built_result = {
        let mut builder = Builder::default()?;

        let result = builder
            .build_target_cwd("//pkg/container:cluster_node_deps", build_config_target)
            .await?;

        result
    };

    println!("Setting hostname to: {}", hostname);
    run_ssh(
        &args.addr,
        &format!("sudo hostnamectl set-hostname {}", hostname),
    )?;

    run_ssh(&args.addr, "sudo mkdir -p /opt/dacha")?;
    run_ssh(
        &args.addr,
        &format!("sudo chown {owner}:{owner} /opt/dacha", owner = ASSET_OWNER),
    )?;

    // Delete any old built artifacts data.
    run_ssh(&args.addr, "sudo rm -rf /opt/dacha/bundle/")?;

    // Cluster cluster data directory
    run_ssh(&args.addr, "mkdir -p /opt/dacha/data")?;
    run_ssh(
        &args.addr,
        &format!(
            "sudo chown {owner}:{owner} /opt/dacha/data",
            owner = NODE_USER
        ),
    )?;
    run_ssh(&args.addr, "sudo chmod 700 /opt/dacha/data")?;

    for (key, value) in node_built_result.outputs.output_files {
        copy_file(&args.addr, value.location, key)?;
    }

    // TODO: This is the only file in the bundle not owned by
    // 'cluster-user:cluster-user'
    // TODO: Find a better place to store this logic that is shared with local
    // executions.
    run_ssh(
        &args.addr,
        &format!(
            "sudo chown root:{} /opt/dacha/bundle/built/pkg/container/newcgroup",
            NODE_USER
        ),
    )?;
    run_ssh(
        &args.addr,
        "sudo chmod 750 /opt/dacha/bundle/built/pkg/container/newcgroup",
    )?;
    run_ssh(
        &args.addr,
        "sudo chmod u+s /opt/dacha/bundle/built/pkg/container/newcgroup",
    )?;

    let mut node_config = {
        let s = file::read_to_string(project_path!("pkg/container/config/node.textproto")).await?;
        NodeConfig::parse_text(&s)?
    };

    node_config.set_zone(args.zone);

    let temp_dir = file::temp::TempDir::create()?;
    let node_config_path = temp_dir.path().join("node.textproto");
    file::write(
        &node_config_path,
        protobuf::text::serialize_text_proto(&node_config),
    )
    .await?;

    // TODO: Generate this with the correct zone.
    copy_file(
        &args.addr,
        &node_config_path,
        "pkg/container/config/node.textproto",
    )?;

    copy_repo_file(&args.addr, "pkg/container/config/node.service")?;

    println!("Setting up /etc/subuid");
    run_ssh(
        &args.addr,
        &format!("echo '{}:400000:65536' | sudo tee /etc/subuid", NODE_USER),
    )?;

    println!("Setting up /etc/subgid");
    {
        let tmpdir = file::temp::TempDir::create()?;
        let groups_path = tmpdir.path().join("group");

        download_file(&args.addr, "/etc/group", groups_path.as_str())?;

        let groups = container::node::shadow::read_groups_from_path(groups_path)?;

        let mut subgid = String::new();
        subgid.push_str(&format!("{}:400000:65536\n", NODE_USER));

        let target_groups = &["gpio", "plugdev", "dialout", "i2c", "spi", "video"];

        for group in groups {
            if target_groups.iter().find(|g| *g == &group.name).is_some() {
                subgid.push_str(&format!("{}:{}:1\n", NODE_USER, group.id));
            }
        }

        println!("{}", subgid);
        println!("");

        let subgid_path = tmpdir.path().join("subgid");
        std::fs::write(&subgid_path, subgid)?;

        run_scp(
            &subgid_path.as_str(),
            &format!("{}@{}:/tmp/next_subgid", ASSET_OWNER, &args.addr),
        )?;

        run_ssh(&args.addr, "sudo cp /tmp/next_subgid /etc/subgid")?;
    }

    // TODO: Also keep other files like /boot/config.txt in sync

    run_ssh(
        &args.addr,
        "sudo systemctl enable /opt/dacha/bundle/pkg/container/config/node.service",
    )?;
    run_ssh(&args.addr, "sudo systemctl start cluster-node")?;

    Ok(())
}
