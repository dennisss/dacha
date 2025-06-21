use alloc::borrow::ToOwned;

use common::errors::*;

use crate::{local::LocalPathBuf, FileErrorKind, LocalFile, LocalPath};

#[derive(Args)]
pub struct CopyCommand {
    #[arg(positional)]
    pub from: LocalPathBuf,

    #[arg(positional)]
    pub to: LocalPathBuf,

    #[arg(default = false)]
    pub recursive: bool,

    #[arg(default = false)]
    pub preserve_metadata: bool,

    pub symlink_root: Option<LocalPathBuf>,

    /// Ignore any files which we can't copy because we don't have permissions
    /// to read them.
    #[arg(default = false)]
    pub skip_permission_denied: bool,
}

pub async fn run_copy_command(cmd: CopyCommand) -> Result<()> {
    // TODO: Handle the '--recursive' flag

    // TODO: Dedup with file::copy_all and file::copy

    if crate::exists(&cmd.to).await? {
        return Err(crate::FileError::new(FileErrorKind::AlreadyExists, "").into());
    }

    let mut relative_paths = vec![];
    relative_paths.push(LocalPath::new("").to_owned());

    while let Some(relative_path) = relative_paths.pop() {
        let from_path = cmd.from.join(&relative_path);
        let to_path = cmd.to.join(&relative_path);

        let meta = crate::symlink_metadata(&from_path).await?;

        // NOTE: We should not attempt to open symlinks as they may have a broken path.
        if !meta.is_symlink() {
            // TODO: Re-use the file handle opened here for future operations.
            if let Err(e) = LocalFile::open(&from_path) {
                if let Some(&sys::Errno::EACCES) = e.downcast_ref() {
                    if cmd.skip_permission_denied {
                        println!("Skip {:?}", from_path);
                        continue;
                    }
                }

                return Err(format_err!("While reading: {:?}: {}", from_path, e));
            }
        }

        if meta.is_dir() {
            crate::create_dir(&to_path).await?;

            for entry in crate::read_dir(&from_path)? {
                relative_paths.push(relative_path.join(entry.name()));
            }
        } else if meta.is_file() {
            crate::copy(&from_path, &to_path).await?;
        } else if meta.is_symlink() {
            let mut link_path = crate::readlink(&from_path)?;

            if let Some(rel_path) = link_path.strip_prefix("/") {
                if let Some(root) = &cmd.symlink_root {
                    link_path = root.join(rel_path);
                }
            }

            // TODO: This will only work well if all paths are normalized.
            if let Some(rel_path) = link_path.strip_prefix(&cmd.from) {
                link_path = cmd.to.join(rel_path);
            }

            if cmd.symlink_root.is_some() && !to_path.join(&link_path).starts_with(&cmd.to) {
                return Err(format_err!("Symlink outside of root: {}", from_path.as_str()));
            }

            crate::symlink(link_path, to_path).await?;
        } else {
            return Err(format_err!("Can't copy {:?}", from_path));
        }

        // TODO: Also do permissions
        // if cmd.preserve_metadata {
        //     crate::chown(&to_path, meta.uid(), meta.gid())?;
        // }
    }

    Ok(())
}
