#[macro_use]
extern crate common;
#[macro_use]
extern crate macros;

use std::io::Read;
use std::sync::Arc;
use std::{fs::File, time::Duration};
use std::time::Instant;

use common::errors::*;
use common::io::{Readable, Writeable};
use executor::bundle::TaskResultBundle;
use file::LocalPathBuf;
use macros::executor_main;
use file::project_path;
use base_args::define_arg_command;

use mocap_tools::*;



#[derive(Args)]
struct Args {
    command: Command
}


define_arg_command!(Command {
    BuildCommand = "build",
    UpdateCommand = "update",
    BackupCommand = "backup"
});



#[executor_main]
async fn main() -> Result<()> {
    let args = common::args::parse_args::<Args>()?;
    args.command.run().await?;
    Ok(())
}