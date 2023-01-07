use crate::{bindings, c_int};

/// Code returned to the parent process when a process ends normally by calling
/// exit().
///
/// Typically a code equal to 0 indicates success.
///
/// NOTE: In Linux, only 8 bit exit codes are supported.
pub type ExitCode = u8;

pub fn exit(code: ExitCode) -> ! {
    unsafe { raw::exit(code) };
    panic!()
}

mod raw {
    use super::*;

    syscall!(exit, bindings::SYS_exit, status: u8 => Infallible<u64>);
}
