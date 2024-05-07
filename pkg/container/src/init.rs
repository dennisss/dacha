use std::time::Duration;

use common::errors::*;
use sys::{sigprocmask, sigsuspend, Errno, Signal, SignalSet, SigprocmaskHow};

pub struct MainProcessOptions {
    /// If true, start the child process in a new process group.
    pub use_setsid: bool,

    /// Extra flags to use when calling clone() to start the child process.
    ///
    /// MUST NOT include CLONE_THREAD or CLONE_VM
    pub clone_flags: sys::CloneFlags,

    /// If true, if we get two SIGINTs while waiting for a child process, the
    /// second SIGINT will be increased to a SIGKILL before being forwarded to
    /// the client.
    pub raise_second_sigint: bool,
}

/// A primary singleton subprocess which is wrapped by the current caller
/// process. Signals, ttys, etc. are forwarded to this subprocess and when the
/// child exits, so will the parent.
///
/// NOTE: This should only be used in a single threaded process with no other
/// signal dependencies. Additionally there should NEVER be more than one
/// instance of this in a single process.
pub struct MainProcess {
    inner: Inner,
}

struct Inner {
    options: MainProcessOptions,
    pid: sys::pid_t,
    dead: bool,
}

impl MainProcess {
    /// Starts a child process running the given code.
    pub fn start<F: FnMut() -> sys::ExitCode>(
        options: MainProcessOptions,
        child_process: F,
    ) -> Result<Self, Errno> {
        unsafe { Self::start_impl(options, child_process) }
    }

    unsafe fn start_impl<F: FnMut() -> sys::ExitCode>(
        options: MainProcessOptions,
        mut child_process: F,
    ) -> Result<Self, Errno> {
        /*
        If using use_setsid(),

            TIOCNOTTY to detach stdin from the parent
                -> Must ignore SIGHUP and SIGCONT (if we are a session leader getsid(0) == getpid())

            then in the child use TIOCSCTTY to attach it
                ^ Errors should be ok.

        */
        // Detach the stdin tty device.
        // TODO: Only do if isatty()
        // if options.use_setsid {
        //     sys::ioctl(0, sys::bindings::TIOCNOTTY, 0)?;
        // }

        // Block all signals in the parent process so that we don't notice signals until
        // we set up signal handlers for it (to avoid race conditions like the child
        // process exiting before we are ready to monitor it).
        sigprocmask(SigprocmaskHow::SIG_BLOCK, Some(&SignalSet::all()), None)?;

        let pid = sys::CloneArgs::new()
            .flags(options.clone_flags)
            .sigchld()
            .spawn_process(|| {
                if let Err(e) = Self::prepare_child(&options) {
                    eprintln!("Failed to initialize child: {}", e);
                    return 1;
                }

                child_process()
            })?;

        Ok(Self {
            inner: Inner {
                options,
                pid,
                dead: false,
            },
        })
    }

    unsafe fn prepare_child(options: &MainProcessOptions) -> Result<(), Errno> {
        // Unblock everything we blocked in the parent.
        sigprocmask(SigprocmaskHow::SIG_UNBLOCK, Some(&SignalSet::all()), None)?;

        // sys::ioctl(0, sys::bindings::TIOCSCTTY, 0)?;

        if options.use_setsid {
            sys::setsid()?;
        }

        Ok(())
    }

    pub fn pid(&self) -> sys::pid_t {
        self.inner.pid
    }

    /// Waits until the child process exits.
    ///
    /// - Any signals received by the init process will be forwarded to the
    ///   child.
    /// - Any subprocesses other than the main child will also be reaped (e.g.
    ///   if the current process has pid == 1).
    /// - Once the main child exits, the init process exits with the same exit
    ///   mode.
    ///
    /// NOTE: This function should normally never return unless there is an
    /// error.
    pub fn wait(mut self) -> Result<()> {
        unsafe { self.inner.wait_impl() }
    }
}

impl Drop for Inner {
    fn drop(&mut self) {
        if !self.dead {
            let _ = unsafe { self.kill(Signal::SIGKILL) };
        }
    }
}

impl Inner {
    unsafe fn kill(&mut self, signal: Signal) -> Result<(), Errno> {
        if signal == Signal::SIGKILL {
            self.dead = true;
        }

        sys::kill(
            if self.options.use_setsid {
                -self.pid
            } else {
                self.pid
            },
            signal,
        )
    }

    unsafe fn wait_impl(&mut self) -> Result<()> {
        let mut got_sigint = false;

        // Wait for signals to occur.
        loop {
            let signal =
                match unsafe { sys::sigtimedwait(SignalSet::all(), Duration::from_secs(1)) } {
                    Ok(v) => Some(v),
                    Err(Errno::EAGAIN) => None, // Timeout
                    Err(e) => return Err(e.into()),
                };

            if signal == Some(Signal::SIGCHLD) {
                loop {
                    let v = sys::waitpid(-1, sys::WaitOptions::WNOHANG)?;
                    match v {
                        sys::WaitStatus::Exited { pid, status } => {
                            self.dead = true;
                            if pid == self.pid {
                                sys::exit(status);
                            }
                        }
                        sys::WaitStatus::Signaled {
                            pid,
                            signal,
                            core_dumped,
                        } => {
                            if pid == self.pid {
                                // TODO: Verify this shows up correctly in waitpid of the parent
                                // process.
                                sys::exit(128 + signal.to_raw() as u8);
                            }
                        }
                        sys::WaitStatus::NoStatus => {
                            break;
                        }
                        _ => {}
                    }
                }
            } else if let Some(mut signal) = signal {
                if signal == sys::Signal::SIGINT || signal == sys::Signal::SIGTERM {
                    if got_sigint && self.options.raise_second_sigint {
                        println!("Killing...");
                        signal = sys::Signal::SIGKILL;
                    }

                    got_sigint = true;
                }

                // Forward signals
                self.kill(signal)?;
            } else {
                // Sometimes we don't get a SIGCHLD for children in separate namespaces so
                // ensure that we still kill them if they become zombies. This
                // mainly happens in the cluster_node top level process. But, poking them with a
                // SIGKILL tends to trigger a SIGCHLD to finally come.

                // This file will look something like:
                // "985987 (cluster_node) Z 985986 985987...."
                let stat = sys::blocking_read_to_string(&format!("/proc/{}/stat", self.pid))?;

                let err_fn = || format_err!("Invalid proc state file: {}", stat);

                // Find the state ('Z' part)
                let state = stat
                    .split_once(") ")
                    .ok_or_else(err_fn)?
                    .1
                    .chars()
                    .next()
                    .ok_or_else(err_fn)?;

                // If a zombie kill it.
                if state == 'Z' {
                    println!("Manually reap zombie...");
                    self.kill(sys::Signal::SIGKILL)?;
                }
            }
        }

        Ok(())
    }
}
