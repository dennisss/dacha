use crate::{bindings, c_int, c_void, pid_t, Errno, Signal};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum WaitStatus {
    /// Process exited normally by calling exit().
    Exited {
        pid: pid_t,
        status: u8,
    },

    /// Process was terminated by a signal.
    Signaled {
        pid: pid_t,
        signal: Signal,
        core_dumped: bool,
    },

    Stopped {
        pid: pid_t,
        signal: Signal,
    },

    /// Process was resummed by a SIGCONT
    Continued {
        pid: pid_t,
    },

    /// No processes have changed state (when WNOHANG was specified in the
    /// waitpid call).
    NoStatus,

    Unknown(pid_t, c_int),
}

impl WaitStatus {
    /// Based on the macros in 'bits/waitstatus.h'.
    pub fn from_value(pid: c_int, wstatus: c_int) -> Self {
        if pid == 0 {
            return Self::NoStatus;
        }

        let status = ((wstatus & 0xff00) >> 8) as u8; // WEXITSTATUS

        if (wstatus & 0x7F) == 0 {
            // WIFEXITED
            Self::Exited { pid, status }
        } else if (wstatus & 0xff) == 0x7f {
            // WIFSTOPPED
            Self::Stopped {
                pid,
                signal: Signal::from_raw(status as u32),
            } // WSTOPSIG
        } else if (((wstatus & 0x7f) + 1) >> 1) > 0 {
            // WIFSIGNALED
            Self::Signaled {
                pid,
                signal: Signal::from_raw((wstatus & 0x7f) as u32), // WTERMSIG
                core_dumped: (wstatus & 0x80 != 0),                // WCOREDUMP
            }
        } else if wstatus == 0xffff {
            Self::Continued { pid }
        } else {
            Self::Unknown(pid, wstatus)
        }
    }
}

define_bit_flags!(WaitOptions c_int {
    WNOHANG = (bindings::WNOHANG as c_int),
    WUNTRACED = (bindings::WUNTRACED as c_int),
    WCONTINUED = (bindings::WCONTINUED as c_int)
});

pub unsafe fn waitpid(pid: pid_t, options: WaitOptions) -> Result<WaitStatus, Errno> {
    let mut wstatus = 0;
    // TODO: Retry EINTR
    let pid = raw::wait4(pid, &mut wstatus, options.to_raw(), core::ptr::null_mut())?;
    Ok(WaitStatus::from_value(pid, wstatus))
}

mod raw {
    use super::*;

    // TODO: Switch last argument to a bindings::rusage
    syscall!(wait4, bindings::SYS_wait4, pid: pid_t, wstatus: *mut c_int, options: c_int, ru: *mut c_void => Result<pid_t>);
}
