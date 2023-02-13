use crate::bindings::{AT_FDCWD, AT_REMOVEDIR, AT_SYMLINK_NOFOLLOW};
use crate::{bindings, c_int, c_uint, Errno};

/// NOTE: Does not produce a null terminator.
pub unsafe fn readlink(path: *const u8, buf: &mut [u8]) -> Result<usize, Errno> {
    raw::readlinkat(AT_FDCWD, path, buf.as_mut_ptr(), buf.len())
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

// AT_FDCWD

// syscall!(rmdir, bindings::SYS_rmdir, path: *const u8 => Result<()>);

pub unsafe fn rmdir(path: *const u8) -> Result<(), Errno> {
    unlinkat(AT_FDCWD, path, AT_REMOVEDIR)
}

pub unsafe fn unlink(path: *const u8) -> Result<(), Errno> {
    unlinkat(AT_FDCWD, path, 0)
}

syscall!(unlinkat, bindings::SYS_unlinkat, fd: c_int, path: *const u8, flags: c_uint => Result<()>);

pub unsafe fn symlink(old: *const u8, new: *const u8) -> Result<(), Errno> {
    symlinkat(old, AT_FDCWD, new)
}

syscall!(symlinkat, bindings::SYS_symlinkat, old: *const u8, newdfd: c_int, new: *const u8 => Result<()>);

pub unsafe fn mkdir(path: *const u8, mode: bindings::mode_t) -> Result<(), Errno> {
    mkdirat(AT_FDCWD, path, mode)
}

// TODO: Use umode_t
syscall!(mkdirat, bindings::SYS_mkdirat, fd: c_int, path: *const u8, mode: bindings::mode_t => Result<()>);

pub unsafe fn stat(path: *const u8, statbuf: *mut bindings::stat) -> Result<(), Errno> {
    fstatat(AT_FDCWD, path, statbuf, 0)
}

pub unsafe fn lstat(path: *const u8, statbuf: *mut bindings::stat) -> Result<(), Errno> {
    fstatat(AT_FDCWD, path, statbuf, AT_SYMLINK_NOFOLLOW)
}

syscall!(fstat, bindings::SYS_fstat, fd: c_int, statbuf: *mut bindings::stat => Result<()>);
syscall!(fstatat, bindings::SYS_newfstatat, dirfd: c_int, path: *const u8, statbuf: *mut bindings::stat, flags: c_uint => Result<()>);

mod raw {
    use super::*;

    syscall!(readlinkat, bindings::SYS_readlinkat, dirfd: c_int, path: *const u8, buf: *mut u8, bufsize: usize => Result<usize>);

    syscall!(flock, bindings::SYS_flock, fd: c_int, operation: c_int => Result<()>);
}

// syscall!(readlink, bindings::SYS_readlink, path: *const u8, buf: *mut u8,
// bufsiz: c_size_t => Result<c_size_t>);
