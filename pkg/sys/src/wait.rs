use crate::c_int;

#[derive(Clone, Copy, Debug)]
pub enum WaitStatus {
    /// Process exited normally by calling exit().
    Exited {
        status: i8,
    },

    /// Process was terminated by a signal.
    Signaled {
        signal: i8,
        core_dumped: bool,
    },

    Stopped {
        signal: i8,
    },

    /// Process was resummed by a SIGCONT
    Continued,

    Unknown(c_int),
}

impl WaitStatus {
    /// Based on the macros in 'bits/waitstatus.h'.
    pub fn from_value(wstatus: c_int) -> Self {
        let status = ((wstatus & 0xff00) >> 8) as i8; // WEXITSTATUS

        if (wstatus & 0x7F) == 0 {
            // WIFEXITED
            Self::Exited { status }
        } else if (((wstatus & 0x7f) + 1) >> 1) > 0 {
            // WIFSIGNALED
            Self::Signaled {
                signal: (wstatus & 0x7f) as i8,     // WTERMSIG
                core_dumped: (wstatus & 0x80 != 0), // WCOREDUMP
            }
        } else if (wstatus & 0xff) == 0x7f {
            // WIFSTOPPED
            Self::Stopped { signal: status } // WSTOPSIG
        } else if wstatus == 0xffff {
            Self::Continued
        } else {
            Self::Unknown(wstatus)
        }
    }
}
