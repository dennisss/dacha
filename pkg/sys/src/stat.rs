use crate::{bindings, c_int, Errno};

/// NOTE: Does not produce a null terminator.
pub unsafe fn readlink(path: *const u8, buf: &mut [u8]) -> Result<usize, Errno> {
    raw::readlink(path, buf.as_mut_ptr(), buf.len())
}

define_transparent_enum!(LockOperation c_int {
    LOCK_SH = (bindings::LOCK_SH as c_int),
    LOCK_EX = (bindings::LOCK_EX as c_int),
    LOCK_UN = (bindings::LOCK_UN as c_int)
});

pub unsafe fn flock(fd: c_int, operation: LockOperation, non_blocking: bool) -> Result<(), Errno> {
    let mut op = operation.to_raw();
    if non_blocking {
        op |= bindings::LOCK_NB as c_int;
    }

    raw::flock(fd, op)
}

syscall!(unlink, bindings::SYS_unlink, path: *const u8 => Result<()>);
syscall!(rmdir, bindings::SYS_unlink, path: *const u8 => Result<()>);

// TODO: Use umode_t
syscall!(mkdir, bindings::SYS_mkdir, path: *const u8, mode: bindings::mode_t => Result<()>);

syscall!(stat, bindings::SYS_stat, path: *const u8, statbuf: *mut bindings::stat => Result<()>);
syscall!(lstat, bindings::SYS_lstat, path: *const u8, statbuf: *mut bindings::stat => Result<()>);
syscall!(fstat, bindings::SYS_stat, fd: c_int, statbuf: *mut bindings::stat => Result<()>);

mod raw {
    use super::*;

    syscall!(readlink, bindings::SYS_readlink, path: *const u8, buf: *mut u8, bufsize: usize => Result<usize>);

    syscall!(flock, bindings::SYS_flock, fd: c_int, operation: c_int => Result<()>);
}

// syscall!(readlink, bindings::SYS_readlink, path: *const u8, buf: *mut u8,
// bufsiz: c_size_t => Result<c_size_t>);
