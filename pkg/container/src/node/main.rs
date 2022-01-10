// This file contains the entrypoint code for the node binary.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::sync::Arc;

use common::async_std::path::Path;
use common::async_std::task;
use common::errors::*;
use nix::mount::MsFlags;
use nix::sched::CloneFlags;
use nix::sys::stat::{umask, Mode};
use nix::unistd::Pid;
use protobuf::text::parse_text_proto;
use rpc_util::AddReflection;

use crate::node::Node;
use crate::node::{shadow::*, NodeContext};
use crate::proto::node::NodeConfig;
use crate::proto::node_service::ContainerNodeIntoService;
use crate::runtime::fd::FileReference;

const MAGIC_STARTUP_BYTE: u8 = 0x88;

#[derive(Args)]
struct Args {
    /// Path to a NodeConfig textproto configuring this node.
    config: String,

    /// Override of the 'zone' field specified in the NodeConfig.
    /// NOTE: This is mainly for use in local testing and should generally not
    /// be used.
    zone: Option<String>,
}

/*
    In the ContainerNodeConfig we should have:

    - username: ""
    - groupname: ""

    TODO: Write disallow to the groups file?
*/

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
    // them later. NOTE: We don't run setgroups() so whatever supplementary
    // groups we already have will be preserved.
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
    println!("Container UID range: {:?}", container_uids);
    println!("Container GID range: {:?}", container_uids);

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

    let (root_pid, mut setup_sender) = spawn_root_process(&node_context, &config)?;

    println!("Root Pid: {}", root_pid.as_raw());

    newuidmap(root_pid.as_raw(), &uidmap)?;
    newgidmap(root_pid.as_raw(), &gidmap)?;

    setup_sender.write_all(&[MAGIC_STARTUP_BYTE])?;
    drop(setup_sender);

    let root_exit = nix::sys::wait::waitpid(root_pid, None)?;

    println!("Root exited: {:?}", root_exit);

    Ok(())
}

fn spawn_root_process(context: &NodeContext, config: &NodeConfig) -> Result<(Pid, std::fs::File)> {
    let (setup_reader_ref, setup_writer_ref) = FileReference::pipe()?;

    let mut stack = [0u8; 6 * 1024 * 1024];
    let mut setup_reader_ref = Some(setup_reader_ref);

    // TODO: Verify that CLONE_NEWNS still allows us to inherit new mounts from the
    // parent namespace.
    let pid = nix::sched::clone(
        Box::new(|| {
            run_root_process(
                context,
                config,
                setup_reader_ref.take().unwrap().open().unwrap(),
            )
        }),
        &mut stack,
        CloneFlags::CLONE_NEWUSER | CloneFlags::CLONE_NEWPID | CloneFlags::CLONE_NEWNS,
        Some(libc::SIGCHLD),
    )?;

    Ok((pid, setup_writer_ref.open()?))
}

fn run_root_process(
    context: &NodeContext,
    config: &NodeConfig,
    setup_reader: std::fs::File,
) -> isize {
    let result = task::block_on(run(context, config, setup_reader));
    let code = match result {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("Container Node Error: {}", e);
            1
        }
    };

    unsafe { libc::exit(code) };
}

async fn run(
    context: &NodeContext,
    config: &NodeConfig,
    mut setup_reader: std::fs::File,
) -> Result<()> {
    let mut done_byte = [0u8; 1];
    setup_reader.read_exact(&mut done_byte)?;
    if &done_byte != &[MAGIC_STARTUP_BYTE] {
        return Err(err_msg("Incorrect startup byte received from parent"));
    }

    nix::unistd::setsid()?;
    if unsafe { libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL) } != 0 {
        return Err(err_msg("Failed to set PR_SET_PDEATHSIG"));
    }

    if unsafe {
        libc::prctl(
            libc::PR_SET_SECUREBITS,
            crate::capabilities::SECBITS_LOCKED_DOWN,
        )
    } != 0
    {
        return Err(err_msg("Failed to set PR_SET_SECUREBITS"));
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
    if !Path::new(config.data_dir()).exists().await {
        return Err(err_msg("Data directory doesn't exist"));
    }

    // Files in the data directory will be created without any group/world
    // permissions. Files which require a less restrictive should be modified on
    // a case-by-base basis.
    umask(Mode::from_bits_truncate(0o077));

    let mut task_bundle = common::bundle::TaskResultBundle::new();

    let node = Node::create(context, config).await?;

    // TODO: Implement shutdown for this.
    task_bundle.add("cluster::Node", node.run());

    let mut server = rpc::Http2Server::new();
    node.add_services(&mut server)?;
    server.add_reflection()?;
    server.set_shutdown_token(common::shutdown::new_shutdown_token());

    task_bundle.add("rpc::Server", server.run(config.service_port() as u16));

    // TODO: Join the task.

    task_bundle.join().await?;

    Ok(())
}
