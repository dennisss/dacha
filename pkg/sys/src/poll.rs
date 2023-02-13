use std::time::Duration;

use crate::{bindings, c_int, c_size_t, c_uint, kernel, Errno};

pub fn poll(fds: &mut [bindings::pollfd], timeout: Option<Duration>) -> Result<usize, Errno> {
    let mut timespec = kernel::timespec::default();
    let timespec_ptr = match timeout {
        Some(v) => {
            timespec = v.into();
            &timespec
        }
        None => core::ptr::null(),
    };

    let n = unsafe {
        raw::ppoll(
            fds.as_mut_ptr(),
            fds.len() as c_uint,
            timespec_ptr,
            core::ptr::null(),
            core::mem::size_of::<kernel::sigset_t>(),
        )
    }? as usize;

    Ok(n)
}

mod raw {
    use super::*;

    // TODO: Retry EINTR
    syscall!(ppoll, bindings::SYS_ppoll,
        ufds: *mut bindings::pollfd,
        nfds: c_uint,
        timeout: *const kernel::timespec,
        sigmask: *const kernel::sigset_t,
        sigsetsize: c_size_t
        => Result<c_uint>);
}
