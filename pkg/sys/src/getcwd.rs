use crate::{bindings, c_ulong, Errno};

/// Returns the number of bytes placed into the buffer (including a null
/// termination byte).
pub unsafe fn getcwd(out: &mut [u8]) -> Result<usize, Errno> {
    raw::getcwd(out.as_mut_ptr(), out.len() as c_ulong)
}

mod raw {
    use super::*;

    syscall!(
        getcwd, bindings::SYS_getcwd, buf: *mut u8, size: c_ulong => Result<usize>
    );
}
