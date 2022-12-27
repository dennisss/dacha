#[macro_use]
extern crate common;

use std::collections::HashMap;
use std::os::unix::prelude::CommandExt;
use std::process::{Command, Stdio};

use common::errors::*;

const BIN_DIR: &'static str = "/home/dennis/workspace/dacha";

struct Binary {
    name: &'static str,
    pass_command: bool,
}

fn main() -> Result<()> {
    let mut args = std::env::args();

    let bin_path = file::current_dir()?.join(args.next().unwrap());
    let bin_dir = bin_path.parent().unwrap();

    let command_name = match args.next() {
        Some(v) => v,
        None => {
            return Err(err_msg(
                "Expected at least one argument specifying the command name",
            ));
        }
    };

    let command_to_binary: HashMap<&'static str, Binary> = map_raw! {
        "build" => Binary { name:  "builder", pass_command: true },
        "cluster" => Binary { name: "cluster", pass_command: false }
    };

    let binary = match command_to_binary.get(command_name.as_str()) {
        Some(v) => v,
        None => {
            return Err(format_err!(
                "No binary registered to handle command: {}",
                command_name
            ));
        }
    };

    let mut cmd = Command::new(bin_dir.join(binary.name));

    cmd.stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());

    if binary.pass_command {
        cmd.arg(command_name);
    }

    Err(cmd.args(args).exec())?;

    Ok(())
}
