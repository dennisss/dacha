/*
Runs as the process 1 in each container. It's job is just to run a single process until competion
while reaping any orphaned processes.

*/

extern crate common;
extern crate libc;
extern crate nix;

use std::ffi::CString;

use common::errors::*;
use nix::sys::{
    signal::{signal, SigHandler, Signal},
    wait::WaitPidFlag,
};

extern "C" fn handle_sigchld(signal: libc::c_int) {}

fn run_child(args: &[CString]) -> ! {
    // NOTE: execv() will reset all of the signal() dispositions, but not any
    // sigprocmask() calls.
    if let Err(e) = nix::unistd::execv(&args[1], &args[2..]) {
        eprintln!("Failed to start child process: {:?}", e);
    }

    std::process::exit(1);
}

fn run() -> Result<i32> {
    {
        let handler = SigHandler::Handler(handle_sigchld);
        unsafe { signal(Signal::SIGCHLD, handler) }?;

        // Ignore termination signals. We assume that they are sent to the entire
        // process group (including our child process).
        unsafe { signal(Signal::SIGINT, SigHandler::SigIgn) }?;
        unsafe { signal(Signal::SIGTERM, SigHandler::SigIgn) }?;
    }

    let mut args = vec![];
    for s in std::env::args() {
        args.push(CString::new(s)?);
    }

    if args.len() < 2 {
        return Err(err_msg("Expected at least argument to init program"));
    }

    let root_pid = match unsafe { nix::unistd::fork() }? {
        nix::unistd::ForkResult::Child => {
            run_child(&args);
        }
        nix::unistd::ForkResult::Parent { child } => child,
    };

    loop {
        let e = nix::sys::wait::waitpid(None, Some(WaitPidFlag::WUNTRACED))?;
        match e {
            nix::sys::wait::WaitStatus::Exited(pid, code) => {
                if pid == root_pid {
                    return Ok(code);
                }
            }
            nix::sys::wait::WaitStatus::Signaled(pid, signal, _) => {
                if pid == root_pid {
                    // TODO: In this case, we should kill ourselves with the same signal to emulate
                    // the death? But will need to disable our signal handlers.

                    eprintln!("Killed by signal {}", signal);
                    return Ok(2);
                }
            }
            nix::sys::wait::WaitStatus::Stopped(pid, _) => {
                if pid == root_pid {
                    eprintln!("Process stopped!");
                    return Ok(3);
                }

                nix::sys::signal::kill(pid, Signal::SIGKILL)?;
            }
            nix::sys::wait::WaitStatus::Continued(_) => {}
            nix::sys::wait::WaitStatus::StillAlive => {
                return Err(err_msg("No progress can be made in init process."))
            }
            nix::sys::wait::WaitStatus::PtraceEvent(_, _, _)
            | nix::sys::wait::WaitStatus::PtraceSyscall(_) => {
                return Err(format_err!("Unhandled process event {:?}", e));
            }
        }
    }
}

fn main() {
    let return_code = match run() {
        Ok(v) => v,
        Err(e) => {
            eprintln!("Init Process Failed: {:?}", e);
            1
        }
    };

    std::process::exit(return_code);
}
