// This file contains the code that runs immediately after the first clone()
// into a container.
// - It is the first code to run in the new namespaces.
// - It is still fully privileged
// - It is responsible for setting up the environment and eventually calling
//   execve().

use std::ffi::CStr;
use std::ffi::CString;
use std::io::Read;
use std::os::unix::fs::symlink;
use std::os::unix::prelude::AsRawFd;
use std::os::unix::prelude::FromRawFd;
use std::os::unix::prelude::IntoRawFd;
use std::path::Path;

use common::errors::*;
use common::failure::ResultExt;
use nix::fcntl::OFlag;
use nix::mount::MsFlags;
use nix::mount::mount;
use nix::pty::PtyMaster;
use nix::pty::posix_openpt;
use nix::sched::CloneFlags;
use nix::sys::signal::SigSet;
use nix::sys::signal::SigmaskHow;
use nix::sys::signal::sigprocmask;
use nix::sys::stat::Mode;
use nix::sys::stat::SFlag;
use nix::sys::stat::makedev;
use nix::sys::stat::mknod;
use nix::sys::stat::umask;
use nix::unistd::Gid;
use nix::unistd::Uid;
use nix::unistd::chown;
use nix::unistd::{dup2, Pid};

use crate::proto::config::*;
use crate::runtime::fd::*;
use crate::capabilities::*;

use super::setup_socket::SetupSocketChild;


// NOTE: You should only pass references as arguments to this and no async_std
// objects can be used in this. e.g. If a async_std::fs::File object is blocked
// in this child process, the Drop handler will block forever waiting for the
// runtime which is frozen in the child process.
pub fn run_child_process(
    container_config: &ContainerConfig,
    container_dir: &Path,
    setup_socket: &mut SetupSocketChild,
    file_mapping: &FileMapping,
) -> isize {
    // TODO: Any failures in this should immediately exit the process.
    // Also how do we stop the normal server stuff from running?

    // TODO: Must ensure that all files are closed.

    let result = run_child_process_inner(container_config, container_dir, setup_socket, file_mapping);
    let status = {
        if let Err(e) = result {
            eprintln!("Child process wrapper failed: {:?}", e);
            1
        } else {
            0
        }
    };

    unsafe { libc::exit(status) };
}


// TODO: Rename the FileMapping to the StdioMapping as we currently only support using it for that purpose.
fn run_child_process_inner(
    container_config: &ContainerConfig,
    container_dir: &Path,
    setup_socket: &mut SetupSocketChild,
    file_mapping: &FileMapping,
) -> Result<()> {
    // Block until the parent is done with setting up our environment.
    setup_socket.wait_user_ns_setup()?;

    // The root directory of the container will be world readable so that the container user can
    // read from it.
    umask(Mode::from_bits_truncate(0o002));

    // Prevent parent processes from seeing the new mounts.
    // But, we will see new mounts created by the parent.
    mount::<str, str, str, str>(
        None,
        "/",
        None,
        MsFlags::MS_SLAVE | MsFlags::MS_REC,
        None,
    )?;

    // Create the directory that we'll use for the new root fs.
    // TODO: Be very explicit about what permission flags should be set on this.
    let root_dir = container_dir.join("root");
    std::fs::create_dir(&root_dir)?;

    /*
    // If joining existing namespaces:

    // TODO: Add CloneFlags::CLONE_NEWTIME
    nix::sched::setns(root_pid_file.as_raw_fd(),
    CloneFlags::CLONE_NEWCGROUP | CloneFlags::CLONE_NEWIPC | CloneFlags::CLONE_NEWNET | CloneFlags::CLONE_NEWNS |
            CloneFlags::CLONE_NEWUSER | CloneFlags::CLONE_NEWUTS)?;

    */

    // If not creating it like this, then we'd want to MS_BIND | MS_REC the
    // root_dir.
    // nix::mount::mount::<str, Path, str, str>(
    //     Some("tmpfs"),
    //     &root_dir,
    //     Some("tmpfs"),
    //     MsFlags::empty(),
    //     None,
    // )?;

    // Bind the root directory to itself so that it becomes a mount point (otherwise we can't mount
    // it as the '/' mount point later).
    mount::<Path, Path, str, str>(
        Some(&root_dir),
        &root_dir,
        None,
        MsFlags::MS_BIND | MsFlags::MS_REC,
        None,
    )?;

    let flag_options = &[
        ("bind", MsFlags::MS_BIND),
        ("nosuid", MsFlags::MS_NOSUID),
        ("noexec", MsFlags::MS_NOEXEC),
        ("nodev", MsFlags::MS_NODEV),
        ("relatime", MsFlags::MS_RELATIME),
        ("ro", MsFlags::MS_RDONLY),
    ];

    for mount in container_config.mounts() {
        let destination = Path::new(mount.destination())
            .strip_prefix("/")
            .map_err(|_| {
                format_err!(
                    "Expected mount destination to be an absolute path but got: {}",
                    mount.destination()
                )
            })?;

        let target = root_dir.join(destination);



        // TODO: Make this an optional step?
        if !target.exists() {
            if let Some(parent_dir) = target.parent() {
                std::fs::create_dir_all(parent_dir)?;
            }
    
            // The mount target must exist. If bind mounting a file or special device,
            // then the target needs to be a file. Otherwise, we'll assume 
            if mount.typ().is_empty() && !Path::new(mount.source()).is_dir() {
                std::fs::write(&target, "")?;
            } else {
                std::fs::create_dir(&target)?;
            }
        }

        let mut flags = MsFlags::empty();
        let mut data = String::new();

        for option in mount.options() {
            let mut found = false;
            for (name, flag) in flag_options {
                if name.eq_ignore_ascii_case(option) {
                    flags |= *flag;
                    found = true;
                    break;
                }
            }

            if !found {
                if !data.is_empty() {
                    data.push(',');
                }
                data.push_str(option);
            }
        }

        nix::mount::mount(
            Some(mount.source()),
            &target,
            Some(mount.typ()),
            flags,
            Some(data.as_str()),
        )
        .with_context(|e| format!("Mount of {:?} failed: {}", mount, e))?;
    }

    // std::fs::create_dir_all(&root_dir.join("dev"))?;
    // std::fs::write(root_dir.join("dev/null"), "")?;

    // // Because we can't use mknod as non-root.
    // nix::mount::mount::<_, _, str, str>(
    //     Some("/dev/null"), &root_dir.join("dev/null"), None,
    //     MsFlags::MS_BIND, None)?;

    // TODO: Move these symlinks to the config.
    {
        let lib_target = std::ffi::CStr::from_bytes_with_nul(b"usr/lib\0")?;
        let lib_linkpath = std::ffi::CString::new(root_dir.join("lib").to_str().unwrap())?;
        let result = unsafe { libc::symlink(lib_target.as_ptr(), lib_linkpath.as_ptr()) };
        if result != 0 {
            return Err(err_msg("Failed to make symlink"));
        }
    }
    {
        let lib_target = std::ffi::CStr::from_bytes_with_nul(b"usr/bin\0")?;
        let lib_linkpath = std::ffi::CString::new(root_dir.join("bin").to_str().unwrap())?;
        let result = unsafe { libc::symlink(lib_target.as_ptr(), lib_linkpath.as_ptr()) };
        if result != 0 {
            return Err(err_msg("Failed to make symlink"));
        }
    }

    {
        symlink("pts/ptmx", root_dir.join("dev/ptmx"))?;
    }

    // println!("MAKE NODE");

    // // TODO: Be explicit about the permissions for this.
    // // Instead just change this to an mknode.
    // mknod(&root_dir.join("dev/null"), SFlag::S_IFCHR, 
    // Mode::from_bits_truncate(0666), makedev(1, 3))?;

    // println!("DONE!");

    mount::<str, Path, str, str>(
        None,
        &root_dir,
        None,
        MsFlags::MS_REMOUNT | MsFlags::MS_BIND | MsFlags::MS_RDONLY,
        None,
    ).with_context(|e| format!("Failed to mount root as read only: {}", e))?;


    // TODO: Also run in the exec case?
    // TODO: Compare to the pivot root here:
    // https://github.com/opencontainers/runc/blob/0d49470392206f40eaab3b2190a57fe7bb3df458/libcontainer/SPEC.md
    {
        nix::unistd::chdir(&root_dir)?;
        nix::mount::mount::<_, _, str, str>(Some(&root_dir), "/", None, MsFlags::MS_MOVE, None)?;
        nix::unistd::chroot(".")?;
        nix::unistd::chdir("/")?;
    }


    // Based on https://man7.org/linux/man-pages/man4/pts.4.html,
    // "major number 5 and minor number 2, usually with mode 0666 and ownership root:root"


    // TODO: Add RDONLY
    // Switch the root mount point back to using MS_SHARED which is usually the default in most
    // linux environments.
    mount::<str, str, str, str>(
        None,
        "/",
        None,
        MsFlags::MS_SHARED | MsFlags::MS_REC,
        None,
    )?;

    exec_child_process(container_config.process(), setup_socket, file_mapping)
}

fn exec_child_process(process: &ContainerProcess, setup_socket: &mut SetupSocketChild, file_mapping: &FileMapping) -> Result<()> {

    ///////////
    // All the post-namespace initialization stuff.

    // Run everything in a separate process group to broadcast signals down.
    // TODO: What if we send a signal to this process before this code runs?
    nix::unistd::setsid()?;

    // Ensure that all signals are unblocked.
    sigprocmask(SigmaskHow::SIG_UNBLOCK, Some(&SigSet::all()), None)?;
    
    if process.args().len() == 0 {
        return Err(err_msg("Expected at least one arg in args list"));
    }

    let mut argv = vec![];
    for arg in process.args() {
        argv.push(CString::new(arg.as_str())?);
    }

    let mut env = vec![];
    for var in process.env() {
        env.push(CString::new(var.as_str())?);
    }

    // Switch to new unpriveleged user.

    let child_uid = Uid::from_raw(process.user().uid());
    let child_gid = Gid::from_raw(process.user().gid());

    let mut additional_gids = vec![ child_gid ];
    for gid in process.user().additional_gids() {
        additional_gids.push(Gid::from_raw(*gid));
    }

    nix::unistd::setgroups(&additional_gids)?;

    nix::unistd::setresuid(child_uid, child_uid, child_uid)?;
    nix::unistd::setresgid(child_gid, child_gid, child_gid)?;
    let _ = nix::unistd::setfsuid(child_uid);
    let _ = nix::unistd::setfsgid(child_gid);

    // Drop all capabilities

    let hdr = cap_user_header {
        version: LINUX_CAPABILITY_VERSION_3,
        pid: unsafe { libc::getpid() }
    };

    // NOTE: This is 2 elements because on 64-bit devices, 64-bit capability sets are supported.
    let data = [
        cap_user_data {
            effective: 0,
            permitted: 0,
            inheritable: 0,
        },
        cap_user_data {
            effective: 0,
            permitted: 0,
            inheritable: 0,
        }
    ];

    {
        // NOTE: This will only work if we are using a user namespace (otherwise we won't have the
        // capabilites needed to change our own capabilities).
        let r = unsafe { libc::syscall(libc::SYS_capset, &hdr, &data) };

        if r != 0 {
            let e = nix::Error::last();
            return Err(format_err!("Failed to drop capabilities with error: {:?}", e));
        }
    }

    let r = unsafe {
        libc::prctl(libc::PR_CAP_AMBIENT, libc::PR_CAP_AMBIENT_CLEAR_ALL, 0, 0, 0)
    };
    if r != 0 {
        return Err(err_msg("Failed to clear ambient capabilities"));
    }

    // TODO: REmove all bounding with PR_CAPBSET_DROP

    // TODO: Remove FS capabilities?

    if process.terminal() {
        // NOTE: This may not support O_CLOEXEC depending on the OS.
        let term_primary = posix_openpt(OFlag::O_RDWR | OFlag::O_CLOEXEC)?;

        let term_secondary_path = unsafe { nix::pty::ptsname(&term_primary) }?;

        // NOTE: The owner of the file will the container's individual user.
        nix::pty::grantpt(&term_primary)?;
        nix::pty::unlockpt(&term_primary)?;

        let term_secondary = nix::fcntl::open(
            std::path::Path::new(&term_secondary_path), OFlag::O_RDWR | OFlag::O_CLOEXEC, Mode::empty())?;
        for i in 0..=2 {
            dup2(term_secondary, i)?;
        }

        // Send the primary end to the parent process.
        setup_socket.send_terminal_fd(unsafe { std::fs::File::from_raw_fd(term_primary.into_raw_fd()) })?;

        // Explicitly closing to make it clear that this file doesn't 
        // drop(term_primary);

    } else {
        for (newfd, file) in file_mapping.iter() {
            // NOTE: Files created using dup2 don't share file descriptor flags, so
            // O_CLOEXEC will be disabled for the target fd.
            let oldfd = unsafe { file.open_raw()? };
            dup2(oldfd, *newfd)?;
    
            // NOTE: We will never end up actually calling close() on the 'oldfd' in
            // te child thread. instead we'll just rely on O_CLOEXEC to get
            // rid of them once we call execve.
        }
    }

    setup_socket.wait_finished()?;

    nix::unistd::execve(&argv[0], &argv, &env)?;

    unsafe { libc::exit(1) };
}
