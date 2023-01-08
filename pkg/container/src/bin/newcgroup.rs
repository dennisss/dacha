//! Binary for creating a cgroup and delegating it to a user/process.
//!
//! For example calling 'newcgroup PID /sys/fs/cgroup/dacha' will:
//! - Create a new cgroup tree at '/sys/fs/cgroup/dacha'
//! - Set the tree's owner to the real user's uid/gid.
//! - Move the process with id PID into that cgroup tree.
//!
//! This is meant to run as root (or more specifically using the set-uid-bit to
//! escalate to root) and depends on file system execute permissions
//! for security.

#[macro_use]
extern crate macros;

use common::errors::*;
use file::LocalPathBuf;

#[derive(Args)]
struct Args {
    #[arg(positional)]
    pid: sys::pid_t,

    /// The cgroup directory to create. Should be an path of the form
    /// '/sys/fs/cgroup/[dir]'.
    ///
    /// If this directory already exists, we will try to delete it. This will
    /// fail if some processes are still running in this group.
    #[arg(positional)]
    cgroup_dir: LocalPathBuf,
}

async fn run() -> Result<()> {
    let uids = sys::getresuid()?;
    let gids = sys::getresgid()?;

    if uids.effective.as_raw() != 0 {
        return Err(err_msg("Expected to be running as root"));
    }

    let args = common::args::parse_args::<Args>()?;

    let dir = args.cgroup_dir.normalized();
    if !dir.starts_with("/sys/fs/cgroup") {
        return Err(err_msg("Only cgroup directories may be created"));
    }

    // Remove any old cgroup tree.
    if file::exists(&dir).await? {
        file::remove_dir_all_with_options(&dir, true).await?;
    }

    // Create the cgroup tree.
    file::create_dir(&dir).await?;

    // Recursively chown
    let owner_uid = uids.real;
    let owner_gid = gids.real;
    file::chown(&dir, owner_uid, owner_gid)?;
    for entry in file::read_dir(&dir)? {
        let path = dir.join(entry.name());
        file::chown(&path, owner_uid, owner_gid)?;
    }

    // Add the process to the cgroup.
    // This effectively moves it out of previous cgroup namespace jail it was in so
    // that it can be moved around inside of this new group later on if needed.
    file::write(dir.join("cgroup.procs"), args.pid.to_string()).await?;

    Ok(())
}

fn main() -> Result<()> {
    // TODO: Just run a single threaded executor.
    executor::run(run())?
}
