//! Utility that can be called by any user to mount a FUSE filesystem.
//!
//! Calling:
//! ./fuse_mount --dir=/mnt/hello --socket_fd=123
//!
//! Will:
//! - Verify that 'dir' is a plain directory owned by the calling user.
//! - Verify that 'socket_fd' refers to an open file descriptor pointing to a
//!   unix socket.
//! - Creates a new FUSE fd (by opening /dev/fuse)
//! - Mounts the FUSE fd at 'dir'
//! - Sends the FUSE fd to the calling processing via an SCM_RIGHTS message to
//!   'socket_fd'.
//!
//! This is meant to run as root (or more specifically using the set-uid-bit to
//! escalate to root).

/*
cargo build --bin fuse_mount
sudo cp target/debug/fuse_mount bin/fuse_mount
sudo chown root:root bin/fuse_mount
sudo chmod 755 bin/fuse_mount
sudo chmod u+s bin/fuse_mount

cargo run --bin fuse

---


*/

#[macro_use]
extern crate macros;

use common::errors::*;
use file::LocalPathBuf;

#[derive(Args)]
struct Args {
    /// Path to the directory at which we want to mount the filesystem.
    dir: LocalPathBuf,

    socket_fd: i32,
}

// TODO: Just run a single threaded executor.
#[executor_main]
async fn main() -> Result<()> {
    let uids = sys::getresuid()?;
    let gids = sys::getresgid()?;

    if uids.effective.as_raw() != 0 {
        return Err(err_msg("Expected to be running as root"));
    }

    let args = common::args::parse_args::<Args>()?;

    // TODO: Make absolute.
    let dir = args.dir.normalized();

    // NOTE: We must unmount before stating the directory since 'stat' will fail if
    // the fuse fs is mounted by the server is dead.
    let existing_mounts = sys::mounts()?;
    for mount in existing_mounts {
        let options = mount.options.split(',').collect::<Vec<_>>();

        if mount.mount_point == dir.as_str()
            && mount.fs_type == "fuse"
            && options.contains(&"user_id=1000")
        {
            sys::umount(&mount.mount_point, sys::UmountFlags::empty())
                .map_err(|e| format_err!("Failed to unmount the old mount point: {}", e))?;
            break;
        }
    }

    let dir_stats = file::metadata_sync(&dir)?;

    if dir_stats.st_uid() != uids.real.as_raw() as u32 {
        return Err(err_msg("dir is not owned by the caller of fuse_mount"));
    }

    if !dir_stats.is_dir() {
        return Err(format_err!("{} is not a directory", dir.as_str()));
    }

    let mut stats = sys::bindings::stat::default();
    unsafe {
        sys::fstat(args.socket_fd, &mut stats)
            .map_err(|e| format_err!("Failed to fstat socket: {}", e))?
    };
    if stats.st_mode & sys::bindings::S_IFSOCK == 0 {
        return Err(err_msg("Passed in FD is not a socket"));
    }

    // TODO: Drop privileges for opening this as in https://github.com/libfuse/libfuse/blob/master/util/fusermount.c#L1184
    let mut fuse_file = file::LocalFile::open_with_options(
        "/dev/fuse",
        &file::LocalFileOpenOptions::new().read(true).write(true),
    )
    .map_err(|e| format_err!("opening fuse file failed: {}", e))?;

    let fuse_fd = unsafe { fuse_file.as_raw_fd() };

    /*
    TODO: Other flags that may be useful:
    default_permissions
    */

    println!("Dir: {}", dir.as_str());
    println!("FD: {}", fuse_fd);

    let data = format!(
        "fd={},rootmode={:o},user_id=1000,group_id=1000",
        fuse_fd,
        sys::bindings::S_IFDIR
    );

    sys::mount(
        Some("fuse"),
        dir.as_str(),
        Some("fuse"),
        sys::MountFlags::MS_NODEV | sys::MountFlags::MS_NOSUID | sys::MountFlags::MS_NOATIME,
        Some(&data),
    )
    .map_err(|e| format_err!("FUSE mount failed: {}", e))?;

    let data = [sys::IoSlice::new(b"FUSE")];
    let control_messages =
        sys::ControlMessageBuffer::new(&[sys::ControlMessage::ScmRights(vec![fuse_fd])]);
    let msg = sys::MessageHeader::new(&data[..], None, Some(&control_messages));

    if sys::sendmsg(args.socket_fd, &msg, 0)
        .map_err(|e| format_err!("Failed to sendmsg FUSE FD: {}", e))?
        != 4
    {
        return Err(err_msg("Failed to send whole sendmsg payload"));
    }

    drop(fuse_file);

    Ok(())
}
