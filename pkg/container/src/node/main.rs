//! This file contains the entrypoint code for the node binary.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::sync::Arc;

use common::errors::*;
use file::{project_path, LocalPath};
use nix::mount::MsFlags;
use nix::sched::CloneFlags;
use nix::sys::stat::{umask, Mode};
use nix::unistd::Pid;
use protobuf::text::parse_text_proto;
use rpc_util::AddReflection;

use crate::init::{MainProcess, MainProcessOptions};
use crate::node::node::{Node, NodeContext};
use crate::node::shadow::*;
use crate::proto::node::NodeConfig;
use crate::proto::node_service::ContainerNodeIntoService;
use crate::runtime::fd::FileReference;
use crate::setup_socket::{SetupSocket, SetupSocketChild, SetupSocketParent};

const START_CHILD_BYTE: u8 = 0x88;

const CGROUP_NAMESPACE_SETUP_BYTE: u8 = 0x89;

const FINISHED_BYTE: u8 = 0x90;

#[derive(Args)]
struct Args {
    /// Path to a NodeConfig textproto configuring this node.
    config: String,

    /// Override of the 'zone' field specified in the NodeConfig.
    /// NOTE: This is mainly for use in local testing and should generally not
    /// be used.
    zone: Option<String>,
}

fn create_idmap(
    subids: Vec<SubordinateIdRange>,
    entity_name: &str,
    entity_id: u32,
) -> Vec<IdMapping> {
    let mut idmap = vec![];

    // Map the current user/group id to the same id in the new namespace.
    idmap.push(IdMapping {
        id: entity_id,
        new_ids: IdRange {
            start_id: entity_id,
            count: 1,
        },
    });

    // Map all allowlisted ranges to the same ranges in the new namespace.
    for range in subids.into_iter().filter(|r| &r.name == entity_name) {
        idmap.push(IdMapping {
            id: range.ids.start_id,
            new_ids: range.ids.clone(),
        });
    }

    idmap
}

fn find_container_ids_range(id_map: &[IdMapping]) -> Result<IdRange> {
    let mut range = None;

    for mapping in id_map {
        if mapping.new_ids.count == 1 {
            continue;
        }

        if !range.is_none() {
            return Err(err_msg("Expected only one id range with more than one id"));
        }

        range = Some(mapping.new_ids.clone());
    }

    let range = range.ok_or_else(|| err_msg("Failed to find a container id range"))?;

    // TODO: Verify that all named users/groups don't exist in the given mapping. It
    // also shouldn't contain the id of the user running the node. (this would
    // imply a potential security risk).

    Ok(range)
}

// TODO: NEed to forward ctrl-c and have graceful shutdown.

pub fn main() -> Result<()> {
    let args = common::args::parse_args::<Args>()?;

    let mut config = NodeConfig::default();
    {
        let config_data = std::fs::read_to_string(args.config)?;
        protobuf::text::parse_text_proto(&config_data, &mut config)?;
    }

    if let Some(zone) = args.zone {
        config.set_zone(zone);
    }

    if config.init_process_args().is_empty() {
        let init_process = project_path!("target/release/container_init");
        if !std::path::Path::new(init_process.as_path()).exists() {
            return Err(err_msg("Missing init process binary"));
        }

        config.add_init_process_args(init_process.to_string());
    }

    let uid = nix::unistd::getresuid()?;
    let gid = nix::unistd::getresgid()?;
    if uid.real.as_raw() == 0 || gid.real.as_raw() == 0 {
        return Err(err_msg("Should not be running as root"));
    }

    let user_entry = read_passwd()?
        .into_iter()
        .find(|e| e.uid == uid.real.as_raw())
        .ok_or_else(|| format_err!("Failed to find passwd entry for uid: {}", uid.real.as_raw()))?;
    println!("Running as user: {}", user_entry.name);

    let all_groups = read_groups()?;

    let group_entry = read_groups()?
        .into_iter()
        .find(|e| e.id == gid.real.as_raw())
        .ok_or_else(|| format_err!("Failed to find group entry for gid: {}", gid.real.as_raw()))?;
    println!("Running as group: {}", group_entry.name);

    // Validate that all ids are consistent and there is no chance of escalating
    // them later.
    nix::unistd::setresuid(uid.real, uid.real, uid.real)?;
    nix::unistd::setresgid(gid.real, gid.real, gid.real)?;
    let _ = nix::unistd::setfsuid(uid.real);
    let _ = nix::unistd::setfsgid(gid.real);

    let uidmap = create_idmap(read_subuids()?, &user_entry.name, uid.real.as_raw());
    let gidmap = create_idmap(read_subgids()?, &group_entry.name, gid.real.as_raw());
    println!("Root UID map: {:?}", uidmap);
    println!("Root GID map: {:?}", gidmap);

    let container_uids = find_container_ids_range(&uidmap)?;
    let container_gids = find_container_ids_range(&gidmap)?;
    println!("Container UID range: {}", container_uids);
    println!("Container GID range: {}", container_uids);

    let local_address = http::uri::Authority {
        user: None,
        host: http::uri::Host::IP(net::local_ip()?),
        port: Some(config.service_port() as u16),
    }
    .to_string()?;
    println!("Starting node on address: {}", local_address);

    let node_context = NodeContext {
        system_groups: all_groups.iter().map(|g| (g.name.clone(), g.id)).collect(),
        sub_uids: uidmap
            .iter()
            .map(|mapping| mapping.new_ids.clone())
            .collect(),
        sub_gids: gidmap
            .iter()
            .map(|mapping| mapping.new_ids.clone())
            .collect(),
        container_uids,
        container_gids,
        local_address,
    };

    let (root_process, mut setup_parent) = spawn_root_process(&node_context, &config)?;

    println!("Root Pid: {}", root_process.pid());

    newuidmap(root_process.pid(), &uidmap)?;
    newgidmap(root_process.pid(), &gidmap)?;

    // Move the root process into its own cgroup.
    newcgroup(root_process.pid(), config.cgroup_dir())
        .map_err(|e| format_err!("While trying to create node's cgroup: {}", e))?;

    setup_parent.notify(START_CHILD_BYTE)?;
    setup_parent.wait(CGROUP_NAMESPACE_SETUP_BYTE)?;

    {
        // Create a sub-group to hold the root process (because cgroups only allows
        // processes in leaf trees).
        let self_group = LocalPath::new(config.cgroup_dir()).join("cluster_node");
        std::fs::create_dir(&self_group).unwrap();

        // Move the root process into the sub-group.
        std::fs::write(
            self_group.join("cgroup.procs"),
            root_process.pid().to_string(),
        )
        .unwrap();

        // Delegate all controllers to the subtree.
        // NOTE: This must be run in the a process not in the cgroup when we are adding
        // 'cpuset'.
        std::fs::write(
            LocalPath::new(config.cgroup_dir()).join("cgroup.subtree_control"),
            "+cpuset +cpu +io +memory +pids",
        )
        .unwrap();
    }

    setup_parent.notify(FINISHED_BYTE)?;

    drop(setup_parent);

    root_process.wait()?;

    Ok(())
}

fn spawn_root_process(
    context: &NodeContext,
    config: &NodeConfig,
) -> Result<(MainProcess, SetupSocketParent)> {
    let (setup_parent, setup_child) = SetupSocket::create()?;

    let mut setup_parent = Some(setup_parent);
    let mut setup_child = Some(setup_child);

    // TODO: Differentiate the parent and child process names

    // TODO: Verify that CLONE_NEWNS still allows us to inherit new mounts from the
    // parent namespace.
    let init_process = MainProcess::start(
        MainProcessOptions {
            use_setsid: true,
            clone_flags: sys::CloneFlags::CLONE_NEWUSER
                | sys::CloneFlags::CLONE_NEWPID
                | sys::CloneFlags::CLONE_NEWNS,
            raise_second_sigint: true,
        },
        || {
            // Close the writer in the child
            setup_parent.take();

            run_root_process(context, config, setup_child.take().unwrap())
        },
    )?;

    // NOTE: The reader will be dropped in the parent on drop().

    Ok((init_process, setup_parent.unwrap()))
}

fn run_root_process(
    context: &NodeContext,
    config: &NodeConfig,
    setup_child: SetupSocketChild,
) -> sys::ExitCode {
    let result = executor::run(run(context, config, setup_child)).unwrap();
    let code = match result {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("Container Node Error: {}", e);
            1
        }
    };

    code
}

async fn run(
    context: &NodeContext,
    config: &NodeConfig,
    mut setup_child: SetupSocketChild,
) -> Result<()> {
    setup_child.wait(START_CHILD_BYTE)?;
    unsafe { sys::unshare(sys::CloneFlags::CLONE_NEWCGROUP).unwrap() };
    setup_child.notify(CGROUP_NAMESPACE_SETUP_BYTE)?;

    setup_child.wait(FINISHED_BYTE)?;

    unsafe {
        sys::prctl(
            sys::PR_SET_PDEATHSIG,
            sys::Signal::SIGKILL.to_raw() as u64,
            0,
            0,
            0,
        )
        .map_err(|e| format_err!("While setting PR_SET_PDEATHSIG: {}", e))?;

        sys::prctl(
            sys::PR_SET_SECUREBITS,
            sys::SECBITS_LOCKED_DOWN as u64,
            0,
            0,
            0,
        )
        .map_err(|e| format_err!("While setting PR_SET_SECUREBITS: {}", e))?;

        sys::prctl(sys::PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0)
            .map_err(|e| format_err!("While setting PR_SET_NO_NEW_PRIVS: {}", e))?;
    }

    // Now that we are in a new PID namespace, we need to re-mount /proc so that all
    // the /proc/[pid] files make sense.
    nix::mount::mount(
        Some("proc"),
        "/proc",
        Some("proc"),
        MsFlags::MS_NOEXEC | MsFlags::MS_NOSUID | MsFlags::MS_NODEV,
        Option::<&str>::None,
    )?;

    // TODO: Create the root directory and set permissions to 600
    // NOTE: This directory should be created with mode 700 where the user running
    // the container node is the owner.
    if !file::exists(LocalPath::new(config.data_dir())).await? {
        return Err(err_msg("Data directory doesn't exist"));
    }

    // Drop all supplementary groups. We will only depend on ones in our gid_map.
    nix::unistd::setgroups(&[])?;

    // Files in the data directory will be created without any group/world
    // permissions. Files which require a less restrictive should be modified on
    // a case-by-base basis.
    umask(Mode::from_bits_truncate(0o077));

    let mut task_bundle = executor::bundle::TaskResultBundle::new();

    let node = Node::create(&context, &config).await?;

    // TODO: Implement shutdown for this.
    task_bundle.add("cluster::Node", node.run());

    let mut server = rpc::Http2Server::new();
    node.add_services(&mut server)?;
    server.add_reflection()?;
    server.set_shutdown_token(executor::signals::new_shutdown_token());

    // TODO: Some of these tasks should be marked as non-blocking so should just be
    // cancelled but not necessary blocked till completion.
    task_bundle.add("rpc::Server", server.run(config.service_port() as u16));

    // TODO: Join the task.

    task_bundle.join().await?;

    Ok(())
}

fn newcgroup(pid: sys::pid_t, dir: &str) -> Result<()> {
    // TODO: Make this a private binary as we don't want a human to directly call
    // it.
    let binary = project_path!("bin/newcgroup").to_string();

    let mut child = std::process::Command::new(&binary)
        .args(&[&pid.to_string(), dir])
        .spawn()?;
    let status = child.wait()?;
    if !status.success() {
        return Err(format_err!("newcgroup exited with failure: {}", status));
    }

    Ok(())
}
