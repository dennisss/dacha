// This file contains the code that runs immediately after the first clone() into a container.
// - It is the first code to run in the new namespaces.
// - It is still fully privileged
// - It is responsible for setting up the environment and eventually calling execve().

use std::ffi::CString;
use std::path::Path;

use common::errors::*;
use common::failure::ResultExt;
use nix::mount::MsFlags;
use nix::unistd::{Pid, dup2};

use crate::proto::config::*;
use crate::runtime::fd::*;

// NOTE: You should only pass references as arguments to this and no async_std objects can be used in this.
// e.g. If a async_std::fs::File object is blocked in this child process, the Drop handler will block forever
// waiting for the runtime which is frozen in the child process.
pub fn run_child_process(container_config: &ContainerConfig, container_dir: &Path, file_mapping: &FileMapping) -> isize {

    // TODO: Any failures in this should immediately exit the process.
    // Also how do we stop the normal server stuff from running?

    // TODO: Must ensure that all files are closed.

    let result = run_child_process_inner(container_config, container_dir, file_mapping);
    let status = {
        if let Err(e) = result {
            eprintln!("Child process wrapper failed: {:?}", e);
            1
        } else {
            0
        }
    };

    status
}

fn run_child_process_inner(
    container_config: &ContainerConfig, container_dir: &Path, file_mapping: &FileMapping
) -> Result<()> {
    // Run everything in a separate process group to broadcast signals down.
    // TODO: What if we send a signal to this process before this code runs?
    // nix::unistd::setsid()?;

    for (newfd, file) in file_mapping.iter() {
        // NOTE: Files created using dup2 don't share file descriptor flags, so O_CLOEXEC will be
        // disabled for the target fd.
        let oldfd = unsafe { file.open_raw()? };
        dup2(oldfd, *newfd)?;

        // NOTE: We will never end up actually calling close() on the 'oldfd' in te child thread.
        // instead we'll just rely on O_CLOEXEC to get rid of them once we call execve.
    }

    // Prevent parent processes from seeing the new mounts.
    nix::mount::mount::<str, str, str, str>(
        None, "/", None, MsFlags::MS_SLAVE | MsFlags::MS_REC, None)?;

    // Create the directory that we'll use for the new root fs.
    let root_dir = container_dir.join("root");
    std::fs::create_dir(&root_dir)?;

    // If not creating it like this, then we'd want to MS_BIND | MS_REC the root_dir.
    nix::mount::mount::<str, Path, str, str>(
        Some("tmpfs"), &root_dir, Some("tmpfs"), MsFlags::empty(), None)?;
    
    
    // TODO: This should no longer be needed right?
    // nix::mount::mount::<Path, Path, str, str>(
    //     Some(&root_dir), &root_dir, None, MsFlags::MS_BIND | MsFlags::MS_PRIVATE | MsFlags::MS_REC, None)?;


    // | MsFlags::MS_RDONLY

    // TODO: Should I also create a new root directory that is mounted as MS_PRIVATE;

    let flag_options = &[
        ("bind", MsFlags::MS_BIND),
        ("nosuid", MsFlags::MS_NOSUID),
        ("noexec", MsFlags::MS_NOEXEC),
        ("nodev", MsFlags::MS_NODEV),
        ("relatime", MsFlags::MS_RELATIME),
        ("ro", MsFlags::MS_RDONLY)
    ];

    for mount in container_config.mounts() {
        let destination = Path::new(mount.destination()).strip_prefix("/")
            .map_err(|_| format_err!("Expected mount destination to be an absolute path but got: {}", mount.destination()))?;

        let target = root_dir.join(destination);

        // TODO: Make this an optional step?
        std::fs::create_dir_all(&target)?;

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
            Some(mount.source()), &target, Some(mount.typ()), flags, Some(data.as_str()))
            .with_context(|e| format!("Mount of {:?} failed: {}", mount, e))?;        
    }

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
        nix::unistd::chdir(&root_dir)?;
        nix::mount::mount::<_, _, str, str>(Some(&root_dir), "/", None, MsFlags::MS_MOVE, None)?;
        nix::unistd::chroot(".")?;
        nix::unistd::chdir("/")?;
    }

    // TODO: Add RDONLY
    nix::mount::mount::<str, str, str, str>(
        None, "/", None, MsFlags::MS_SHARED | MsFlags::MS_REC, None)?;

    // TODO: Relinquish capabilities and do setuid and setgid. and effective stuff.

    // TODO: Make sure that SIGINT is forwarded to the child instead of to the parent.

    if container_config.process().args().len() == 0 {
        return Err(err_msg("Expected at least one arg in args list"));
    }

    let mut argv = vec![];
    for arg in container_config.process().args() {
        argv.push(CString::new(arg.as_str())?);
    }

    let mut env = vec![];
    for var in container_config.process().env() {
        env.push(CString::new(var.as_str())?);
    }

    nix::unistd::execve(&argv[0], &argv, &env)?;

    unsafe { libc::exit(1) };
}