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
struct RealPathCommand {
    #[arg(positional)]
    path: LocalPathBuf,
}

async fn run_realpath_command(cmd: RealPathCommand) -> Result<()> {
    println!("{}", file::realpath(cmd.path).await?.as_str());

    Ok(())
}

#[executor_main]
async fn main() -> Result<()> {
    let args = common::args::parse_args::<Args>()?;

    match args.command {
        Command::Copy(cmd) => file::run_copy_command(cmd).await,
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
