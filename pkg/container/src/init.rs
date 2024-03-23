use nix::sys::signal::Signal;
use nix::sys::signal::{sigaction, SaFlags, SigAction, SigHandler, SigSet};
use sys::{sigprocmask, sigsuspend, Errno, SignalSet, SigprocmaskHow};

static mut RECEIVED_SIGNAL: Option<sys::c_int> = None;

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
    pid: sys::pid_t,
    options: MainProcessOptions,
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

        Ok(Self { options, pid })
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
        self.pid
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
    pub fn wait(self) -> Result<(), Errno> {
        unsafe { self.wait_impl() }
    }

    unsafe fn wait_impl(&self) -> Result<(), Errno> {
        // Configure a handler for all signals.
        // Signal numbers 1 to 31 as normal signals (there may be holes though). Above
        // that are realtime signals.
        {
            let action = SigAction::new(
                SigHandler::Handler(handle_signal),
                SaFlags::empty(),
                SigSet::all(),
            );

            let mut some_succeeded = false;
            for signal_num in 1..=31 {
                some_succeeded |= sigaction(core::mem::transmute(signal_num), &action).is_ok();
            }

            // Some may fail as not all systems have all 31 syscall numbers defined.
            if !some_succeeded {
                return Err(Errno::EIO);
            }
        }

        let mut got_sigint = false;

        // Wait for signals to occur.
        loop {
            // Unmask all signals while waiting.
            sys::sigsuspend(&SignalSet::empty());

            let mut signal = match RECEIVED_SIGNAL.take() {
                Some(num) => sys::Signal::from_raw(num as u32),
                None => continue,
            };

            if signal == sys::Signal::SIGCHLD {
                loop {
                    let v = sys::waitpid(-1, sys::WaitOptions::WNOHANG)?;
                    match v {
                        sys::WaitStatus::Exited { pid, status } => {
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
            } else {
                if signal == sys::Signal::SIGINT {
                    if got_sigint && self.options.raise_second_sigint {
                        println!("Killing...");
                        signal = sys::Signal::SIGKILL;
                    }

                    got_sigint = true;
                }

                // Forward signals
                sys::kill(
                    if self.options.use_setsid {
                        -self.pid
                    } else {
                        self.pid
                    },
                    signal,
                )?;
            }
        }

        Ok(())
    }
}

extern "C" fn handle_signal(signal: sys::c_int) {
    unsafe {
        assert!(RECEIVED_SIGNAL.is_none());
        RECEIVED_SIGNAL = Some(signal);
    }
}
