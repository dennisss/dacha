#[macro_use]
extern crate macros;

use common::{errors::*, io::Readable};
use file::{FileErrorKind, LocalFile, LocalPath, LocalPathBuf};

#[derive(Args)]
struct Args {
    command: Command,
}

#[derive(Args)]
enum Command {
    #[arg(name = "copy")]
    Copy(CopyCommand),

    #[arg(name = "realpath")]
    RealPath(RealPathCommand),
}

#[derive(Args)]
struct CopyCommand {
    #[arg(positional)]
    from: LocalPathBuf,

    #[arg(positional)]
    to: LocalPathBuf,

    #[arg(default = false)]
    recursive: bool,

    #[arg(default = false)]
    preserve_metadata: bool,

    symlink_root: Option<LocalPathBuf>,

    /// Ignore any files which we can't copy because we don't have permissions
    /// to read them.
    #[arg(default = false)]
    skip_permission_denied: bool,
}

#[derive(Args)]
struct RealPathCommand {
    #[arg(positional)]
    path: LocalPathBuf,
}

async fn run_copy_command(cmd: CopyCommand) -> Result<()> {
    // TODO: Handle the '--recursive' flag

    // TODO: Dedup with file::copy_all and file::copy

    if file::exists(&cmd.to).await? {
        return Err(file::FileError::new(FileErrorKind::AlreadyExists, "").into());
    }

    let mut relative_paths = vec![];
    relative_paths.push(LocalPath::new("").to_owned());

    while let Some(relative_path) = relative_paths.pop() {
        let from_path = cmd.from.join(&relative_path);
        let to_path = cmd.to.join(&relative_path);

        let meta = file::symlink_metadata(&from_path).await?;

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
            file::create_dir(&to_path).await?;

            for entry in file::read_dir(&from_path)? {
                relative_paths.push(relative_path.join(entry.name()));
            }
        } else if meta.is_file() {
            file::copy(&from_path, &to_path).await?;
        } else if meta.is_symlink() {
            let mut link_path = file::readlink(&from_path)?;

            if let Some(rel_path) = link_path.strip_prefix("/") {
                if let Some(root) = &cmd.symlink_root {
                    link_path = root.join(rel_path);
                }
            }

            file::symlink(link_path, to_path).await?;
        } else {
            return Err(format_err!("Can't copy {:?}", from_path));
        }

        // TODO: Also do permissions
        // if cmd.preserve_metadata {
        //     file::chown(&to_path, meta.uid(), meta.gid())?;
        // }
    }

    Ok(())
}

async fn run_realpath_command(cmd: RealPathCommand) -> Result<()> {
    println!("{}", file::realpath(cmd.path).await?.as_str());

    Ok(())
}

#[executor_main]
async fn main() -> Result<()> {
    let args = common::args::parse_args::<Args>()?;

    match args.command {
        Command::Copy(cmd) => run_copy_command(cmd).await,
        Command::RealPath(cmd) => run_realpath_command(cmd).await,
    }

    /*
    println!("{:#?}", file::read_dir(".")?);

    println!("{:?}", file::readlink("built")?);

    let mut file = ::file::LocalFile::open("hello_world")?;

    let mut buf = vec![];
    file.read_to_end(&mut buf).await?;

    println!("{:?}", std::str::from_utf8(&buf)?);

    Ok(())
     */
}
