#![feature(c_size_t, cstr_from_bytes_until_nul)]

#[macro_use]
extern crate common;
#[macro_use]
extern crate regexp_macros;
extern crate elf;

#[macro_use]
mod macros;

#[macro_use]
mod syscall;

mod capabilities;
mod clone;
mod credentials;
// mod epoll;
mod errno;
mod exit;
mod file;
mod getcwd;
mod getdents;
mod io_uring;
mod iov;
mod kernel;
mod mapped_memory;
mod poll;
mod proc;
mod send;
mod signal;
mod socket;
mod stat;
// pub mod thread;
mod mount;
mod utils;
pub mod utsname;
mod virtual_memory;
mod wait;

pub mod bindings {
    #![allow(non_upper_case_globals)]
    #![allow(non_camel_case_types)]
    #![allow(non_snake_case)]

    include!(concat!(env!("OUT_DIR"), "/bindings.rs"));
}

pub use capabilities::*;
pub use clone::*;
pub use core::ffi::{c_size_t, c_ssize_t, c_uchar, c_void};
pub use credentials::*;
// pub use epoll::*;
pub use errno::*;
pub use exit::*;
pub use file::OpenFileDescriptor;
pub use getcwd::*;
pub use getdents::*;
pub use io_uring::*;
pub use iov::*;
pub use mapped_memory::*;
pub use mount::*;
pub use poll::*;
pub use proc::*;
pub use send::*;
pub use signal::*;
pub use socket::*;
pub use stat::*;
pub use std::os::raw::{c_char, c_int, c_short, c_uint, c_ulong, c_ushort};
pub use virtual_memory::*;
pub use wait::*;

/// Integer of the same width as a 'void *'.
pub type uintptr_t = c_size_t;

/// Verifies at compile time that uintptr_t is correct.
const UINTPTR_TEST: uintptr_t = unsafe { core::mem::transmute(core::ptr::null::<c_void>()) };

pub type umode_t = c_ushort;

// TODO: Check this.
pub type off_t = c_size_t;

// TODO: Check this.
// This should be 32bit.
pub type pid_t = c_int;

pub const SEEK_SET: c_uint = 0;

pub use bindings::{
    pollfd, O_APPEND, O_CLOEXEC, O_CREAT, O_EXCL, O_NONBLOCK, O_RDONLY, O_RDWR, O_SYNC, O_TRUNC,
    O_WRONLY,
};

pub const O_DIRECT: u32 = 0o00040000;

pub const O_DIRECTORY: u32 = 0o00200000;

pub use bindings::{
    CLONE_FILES, CLONE_FS, CLONE_IO, CLONE_SETTLS, CLONE_SIGHAND, CLONE_THREAD, CLONE_VM,
};

// pub use bindings::{ARCH_GET_FS, ARCH_SET_FS};

pub use kernel::{
    PR_CAP_AMBIENT, PR_GET_NO_NEW_PRIVS, PR_SET_NO_NEW_PRIVS, PR_SET_PDEATHSIG, PR_SET_SECUREBITS,
};

macro_rules! export_cast_bindings {
    ($t:ty, $($name:ident),*) => {
        $(
            pub const $name: $t = $crate::bindings::$name as $t;
        )*
    };
}

// Should match the pollfd::events field.
export_cast_bindings!(c_short, POLLERR, POLLHUP, POLLIN, POLLNVAL, POLLOUT);

/*
See also nice list of syscalls here:
https://chromium.googlesource.com/chromiumos/docs/+/HEAD/constants/syscalls.md


*/

syscall!(read, bindings::SYS_read, fd: c_int, buf: *mut u8, count: c_size_t => Result<c_size_t>);
syscall!(write, bindings::SYS_write, fd: c_int, buf: *const u8, count: c_size_t => Result<c_size_t>);
// syscall!(open, bindings::SYS_open, path: *const c_char, flags: c_uint, mode:
// umode_t => Result<c_int>);

pub unsafe fn open(path: *const c_char, flags: c_uint, mode: umode_t) -> Result<c_int, Errno> {
    openat(bindings::AT_FDCWD, path, flags, mode)
}

syscall!(openat, bindings::SYS_openat, fd: c_int, filename: *const c_char, flags: c_uint, mode: umode_t => Result<c_int>);
syscall!(close, bindings::SYS_close, fd: c_int => Result<()>);
syscall!(lseek, bindings::SYS_lseek, fd: c_int, offset: off_t, whence: c_uint => Result<off_t>);

pub unsafe fn fork() -> Result<Option<pid_t>, Errno> {
    let pid = CloneArgs::new().sigchld().run()?;
    Ok(if pid != 0 { Some(pid) } else { None })
}

syscall!(fsync, bindings::SYS_fsync, fd: c_int => Result<()>);
syscall!(fdatasync, bindings::SYS_fdatasync, fd: c_int => Result<()>);
syscall!(ftruncate, bindings::SYS_ftruncate, fd: c_int, len: u64 => Result<()>);

syscall!(fchmod, bindings::SYS_fchmod, fd: c_int, mode: bindings::mode_t => Result<()>);
syscall!(fchmodat, bindings::SYS_fchmodat, dirfd: c_int, path: *const u8, mode: bindings::mode_t, flags: c_uint => Result<()>);

pub unsafe fn chmod(path: *const u8, mode: bindings::mode_t) -> Result<(), Errno> {
    fchmodat(bindings::AT_FDCWD, path, mode, 0)
}

// syscall!(chmod, bindings::SYS_chmod, path: *const u8, mode: bindings::mode_t
// => Result<()>);

pub unsafe fn rename(oldname: *const c_char, newname: *const c_char) -> Result<(), Errno> {
    renameat2(bindings::AT_FDCWD, oldname, bindings::AT_FDCWD, newname, 0)
}

// TODO: Take advantage of the flags for performing atomic swaps or
// replacements.
syscall!(renameat2, bindings::SYS_renameat2,
    olddirfd: c_int, oldname: *const c_char,
    newdirfd: c_int, newname: *const c_char,
    flags: c_uint => Result<()>);

syscall!(ioctl, bindings::SYS_ioctl, fd: c_uint, cmd: c_uint, arg: c_ulong => Result<c_int>);

syscall!(perf_event_open, bindings::SYS_perf_event_open,
    attr: *const bindings::perf_event_attr, pid: pid_t, cpu: c_int, group_fd: c_int, flags: c_ulong => Result<c_int>);

// NOTE: This technically has 3 arguments but the third one is never used in the
// kernel.
syscall!(getcpu, bindings::SYS_getcpu, cpu: *mut c_uint, node: *mut c_uint => Result<()>);

syscall!(getpid, bindings::SYS_getpid => Infallible<pid_t>);
syscall!(getppid, bindings::SYS_getppid => Infallible<pid_t>);
syscall!(gettid, bindings::SYS_gettid => Infallible<pid_t>);

syscall!(getsid, bindings::SYS_getsid => Result<pid_t>);
syscall!(setsid, bindings::SYS_setsid => Result<pid_t>);

// syscall!(eventfd, bindings::SYS_eventfd, count: c_uint => Result<c_int>);
syscall!(eventfd2, bindings::SYS_eventfd2, count: c_uint, flags: c_uint => Result<c_int>);

syscall!(
    prctl, bindings::SYS_prctl, option: c_uint, arg2: c_ulong, arg3: c_ulong, arg4: c_ulong, arg5: c_ulong => Result<u64>
);

/*
syscall!(
    arch_prctl_set, bindings::SYS_arch_prctl, code: c_int, addr: c_ulong => Result<()>
);

syscall!(
    arch_prctl_get, bindings::SYS_arch_prctl, code: c_uint, addr: *mut c_ulong => Result<()>
);
*/
