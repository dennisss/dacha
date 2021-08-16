// This file contains the entrypoint code for the node binary.

use std::io::{Read, Write};
use std::sync::Arc;

use common::async_std::path::Path;
use common::errors::*;
use common::async_std::task;
use nix::mount::MsFlags;
use nix::sys::stat::{Mode, umask};
use nix::unistd::Pid;
use protobuf::text::parse_text_proto;
use nix::sched::CloneFlags;

use crate::proto::service::ContainerNodeIntoService;
use crate::node::Node;
use crate::runtime::fd::FileReference;

const MAGIC_STARTUP_BYTE: u8 = 0x88;

struct PasswdEntry {
    name: String,
    password: String,
    uid: u32,
    gid: u32,
    comment: String,
    directory: String,
    shell: String
}

fn read_passwd() -> Result<Vec<PasswdEntry>> {
    let mut out = vec![];
    let data = std::fs::read_to_string("/etc/passwd")?;
    for line in data.lines() {
        let fields = line.split(":").collect::<Vec<_>>();
        if fields.len() != 7 {
            return Err(format_err!("Incorrect number of fields in passwd line: \"{}\"", line));
        }

        out.push(PasswdEntry {
            name: fields[0].to_string(),
            password: fields[1].to_string(),
            uid: fields[2].parse()?,
            gid: fields[3].parse()?,
            comment: fields[4].to_string(),
            directory: fields[5].to_string(),
            shell: fields[6].to_string()
        });
    }

    Ok(out)
}

struct GroupEntry {
    name: String,
    password: String,
    id: u32,
    user_list: Vec<String>
}

fn read_groups() -> Result<Vec<GroupEntry>> {
    let mut out = vec![];
    let data = std::fs::read_to_string("/etc/group")?;
    for line in data.lines() {
        let fields = line.split(":").collect::<Vec<_>>();
        if fields.len() != 4 {
            return Err(format_err!("Incorrect number of fields in group line: \"{}\"", line));
        }

        out.push(GroupEntry {
            name: fields[0].to_string(),
            password: fields[1].to_string(),
            id: fields[2].parse()?,
            user_list: fields[3].split(",").map(|s| s.to_string()).collect()
        });
    }

    Ok(out)
}



struct SubordinateIdRange {
    name: String,
    start_id: u32,
    count: u32
}

fn read_subordinate_id_file(path: &str) -> Result<Vec<SubordinateIdRange>>  {
    let mut out = vec![];

    let data = std::fs::read_to_string(path)?;
    for line in data.lines() {
        let fields = line.split(":").collect::<Vec<_>>();
        if fields.len() != 3 {
            return Err(format_err!("Incorrect number of fields in sub id line: \"{}\"", line));
        }

        out.push(SubordinateIdRange {
            name: fields[0].to_string(),
            start_id: fields[1].parse()?,
            count: fields[2].parse()?
        });
    }

    Ok(out)
}

/*
    In the ContainerNodeConfig we should have:
    
    - username: ""
    - groupname: ""

    TODO: Write disallow to the groups file?
*/

// newuidmap <pid> <uid> <loweruid> <count> [ <uid> <loweruid> <count> ] ...
// /usr/bin/newuidmap
fn newidmap(binary: &str, sub_ids_path: &str, entity_name: &str, pid: i32) -> Result<()> {
    let mut args = vec![];
    args.push(pid.to_string());

    args.push("1000".to_string());
    args.push("1000".to_string());
    args.push("1".to_string());

    let subids = read_subordinate_id_file(sub_ids_path)?;
    for range in subids {
        if range.name != entity_name {
            continue;
        }

        args.push(range.start_id.to_string());
        args.push(range.start_id.to_string());
        args.push(range.count.to_string());
    }
    
    let mut child = std::process::Command::new(binary)
        .args(&args)
        .spawn()?;
    let status = child.wait()?;
    if !status.success() {
        return Err(format_err!("{} exited with failure: {:?}", binary, status));
    }

    Ok(())
}


pub fn main() -> Result<()> {
    let uid = nix::unistd::getresuid()?;
    let gid = nix::unistd::getresgid()?;
    if  uid.real.as_raw() == 0 || gid.real.as_raw() == 0 {
        return Err(err_msg("Should not be running as root"));
    }

    let user_entry = read_passwd()?
        .into_iter().find(|e| e.uid == uid.real.as_raw())
        .ok_or_else(|| format_err!("Failed to find passwd entry for uid: {}", uid.real.as_raw()))?;
    println!("Running as user: {}", user_entry.name);

    let group_entry = read_groups()?
        .into_iter().find(|e| e.id == gid.real.as_raw())
        .ok_or_else(|| format_err!("Failed to find group entry for gid: {}", gid.real.as_raw()))?;
    println!("Running as group: {}", group_entry.name);

    // Validate that all ids are consistent and there is no chance of escalating them later.
    // NOTE: We don't run setgroups() so whatever supplementary groups we already have will be preserved. 
    nix::unistd::setresuid(uid.real, uid.real, uid.real)?;
    nix::unistd::setresgid(gid.real, gid.real, gid.real)?;
    let _ = nix::unistd::setfsuid(uid.real);
    let _ = nix::unistd::setfsgid(gid.real);

    let (root_pid, mut setup_sender) = spawn_root_process()?;

    println!("Root Pid: {}", root_pid.as_raw());

    newidmap("newuidmap", "/etc/subuid", &user_entry.name, root_pid.as_raw())?;
    newidmap("newgidmap", "/etc/subgid", &group_entry.name, root_pid.as_raw())?;

    setup_sender.write_all(&[MAGIC_STARTUP_BYTE])?;
    drop(setup_sender);

    let root_exit = nix::sys::wait::waitpid(root_pid, None)?;

    println!("Root exited: {:?}", root_exit);

    Ok(())
}

fn spawn_root_process() -> Result<(Pid, std::fs::File)> {
    let (setup_reader_ref, setup_writer_ref) = FileReference::pipe()?;

    let mut stack = [0u8; 6*1024*1024];
    let mut setup_reader_ref = Some(setup_reader_ref);

    // TODO: Verify that CLONE_NEWNS still allows us to inherit new mounts from the parent namespace.
    let pid = nix::sched::clone(
        Box::new(|| run_root_process(setup_reader_ref.take().unwrap().open().unwrap())),
        &mut stack,
        CloneFlags::CLONE_NEWUSER | CloneFlags::CLONE_NEWPID | CloneFlags::CLONE_NEWNS,
        Some(libc::SIGCHLD),
    )?;

    Ok((pid, setup_writer_ref.open()?))
}

fn run_root_process(setup_reader: std::fs::File) -> isize {
    let result = task::block_on(run(setup_reader));
    let code = match result {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("Container Node Error: {}", e);
            1
        }
    };
    
    unsafe { libc::exit(code) };
}

async fn run(mut setup_reader: std::fs::File) -> Result<()> {
    let mut done_byte = [0u8; 1];
    setup_reader.read_exact(&mut done_byte)?;
    if &done_byte != &[MAGIC_STARTUP_BYTE] {
        return Err(err_msg("Incorrect startup byte received from parent"));
    }

    nix::unistd::setsid()?;
    if unsafe { libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL) } != 0 {
        return Err(err_msg("Failed to set PR_SET_PDEATHSIG"));
    }

    if unsafe { libc::prctl(libc::PR_SET_SECUREBITS, crate::capabilities::SECBITS_LOCKED_DOWN) } != 0 {
        return Err(err_msg("Failed to set PR_SET_SECUREBITS"));
    }

    // Now that we are in a new PID namespace, we need to re-mount /proc so that all the /proc/[pid]
    // files make sense.
    nix::mount::mount(Some("proc"), "/proc", Some("proc"),
    MsFlags::MS_NOEXEC | MsFlags::MS_NOSUID | MsFlags::MS_NODEV, Option::<&str>::None)?;

    println!("Done setup!");

    // NOTE: This directory should be created with mode 700 where the user running the container
    // node is the owner. 
    if !Path::new("/opt/dacha").exists().await {
        return Err(err_msg("Data directory doesn't exist"));
    }

    // Files in the data directory will be created without any group/world permissions.
    // Files which require a less restrictive should be modified on a case-by-base basis. 
    umask(Mode::from_bits_truncate(0o077));


    println!("Starting node!");


    // TODO: Create the root directory and set permissions to 600


    let node = Node::create().await?;

    // TODO: If this fails, trigger immediate server cancellation as we
    // can't really procees any requests when this fails.
    let task_handle = task::spawn(node.run());

    let mut server = rpc::Http2Server::new();
    server.add_service(node.into_service())?;
    server.set_shutdown_token(common::shutdown::new_shutdown_token());
    server.run(8080).await?;

    // TODO: Join the task.

    Ok(())
}
