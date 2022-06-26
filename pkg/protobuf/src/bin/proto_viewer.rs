extern crate common;
extern crate protobuf;
#[macro_use]
extern crate macros;

use common::async_std::fs;
use common::errors::*;

#[derive(Args)]
struct Args {
    #[arg(positional)]
    path: String,
}

async fn run() -> Result<()> {
    let args = common::args::parse_args::<Args>()?;

    let data = fs::read(std::env::current_dir()?.join(&args.path)).await?;

    protobuf::viewer::print_message(&data, "\t")?;

    Ok(())
}

fn main() -> Result<()> {
    common::async_std::task::block_on(run())
}
