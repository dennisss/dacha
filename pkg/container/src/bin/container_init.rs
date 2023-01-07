//! Binary meant to run as pid 1 in a container.
//!
//! This is similar to https://github.com/Yelp/dumb-init
//!
//! It's responsible for starting and waiting on a subprocess while:
//! - Proxying signals and exit codes.
//! - Reaping zombie processes.

extern crate common;
extern crate nix;
extern crate sys;
#[macro_use]
extern crate macros;

use std::ffi::CString;

use common::{args::list::EscapedArgs, errors::*};
use container::init::{MainProcess, MainProcessOptions};

#[derive(Args)]
struct Args {
    sub_command: EscapedArgs,
}

fn run() -> Result<()> {
    let args = common::args::parse_args::<Args>()?;

    // TODO: Check how many file descriptors we have to verify none were leaked
    // without CLOEXEC.

    let mut sub_command = vec![];
    for arg in args.sub_command.args {
        sub_command.push(CString::new(arg)?);
    }

    if sub_command.len() < 1 {
        return Err(err_msg(
            "Expected at least one argument for the sub command to run",
        ));
    }

    let main_process = MainProcess::start(
        MainProcessOptions {
            use_setsid: true,
            clone_flags: sys::CloneFlags::empty(), // Basically use fork()
            raise_second_sigint: true,
        },
        || {
            // TODO: Ensure that this forwards all environment variables.
            if let Err(e) = nix::unistd::execv(&sub_command[0], &sub_command[0..]) {
                eprintln!("Failed to start child process: {:?}", e);
            }

            1 // exit code
        },
    )?;

    main_process.wait()?;

    // Should never be reached
    Ok(())
}

fn main() {
    eprintln!(
        "Init Process Failed: {}",
        run()
            .err()
            .unwrap_or_else(|| err_msg("Failed to wait for main process of container"))
    );

    sys::exit(1);
}
