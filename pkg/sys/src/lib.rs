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

mod epoll;
mod errno;
mod file;
mod io_uring;
mod iov;
mod mapped_memory;
mod num_cpus;
mod proc;
// mod signal;
mod kernel;
mod socket;
pub mod thread;
pub mod utsname;
mod virtual_memory;
mod wait;

pub mod bindings {
    #![allow(non_upper_case_globals)]
    #![allow(non_camel_case_types)]
    #![allow(non_snake_case)]

    include!(concat!(env!("OUT_DIR"), "/bindings.rs"));
}

pub use core::ffi::{c_size_t, c_ssize_t, c_uchar, c_void};
pub use epoll::*;
pub use errno::*;
pub use io_uring::*;
pub use iov::*;
pub use mapped_memory::*;
pub use num_cpus::*;
pub use proc::*;
// pub use signal::*;
pub use file::OpenFileDescriptor;
pub use socket::*;
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

pub use bindings::{pollfd, O_CLOEXEC, O_NONBLOCK, O_RDONLY, O_RDWR};

pub use bindings::{
    CLONE_FILES, CLONE_FS, CLONE_IO, CLONE_SETTLS, CLONE_SIGHAND, CLONE_THREAD, CLONE_VM,
};

pub use bindings::{ARCH_GET_FS, ARCH_SET_FS};

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
syscall!(open, bindings::SYS_open, path: *const c_char, flags: c_uint, mode: umode_t => Result<c_int>);
syscall!(close, bindings::SYS_close, fd: c_int => Result<()>);
syscall!(lseek, bindings::SYS_lseek, fd: c_int, offset: off_t, whence: c_uint => Result<off_t>);

#[cfg(target_arch = "x86_64")]
pub unsafe fn clone(
    flags: c_uint,
    stack: *mut c_void,
    parent_tid: *mut c_int,
    child_tid: *mut c_int,
    tls: c_ulong,
) -> Result<pid_t, Errno> {
    syscall!(
        clone_raw,
        bindings::SYS_clone,
        flags: c_uint, // c_ulong,
        stack: *mut c_void,
        parent_tid: *mut c_int,
        child_tid: *mut c_int,
        tls: c_ulong
        => Result<pid_t>
    );

    clone_raw(flags, stack, parent_tid, child_tid, tls)
}

// See the 'clone' man page for this.
#[cfg(target_arch = "aarch64")]
pub unsafe fn clone(
    flags: c_uint,
    stack: *mut c_void,
    parent_tid: *mut c_int,
    child_tid: *mut c_int,
    tls: c_ulong,
) -> Result<pid_t, Errno> {
    syscall!(
        clone_raw,
        bindings::SYS_clone,
        flags: c_uint, // c_ulong,
        stack: *mut c_void,
        parent_tid: *mut c_int,
        tls: c_ulong,
        child_tid: *mut c_int
        => Result<pid_t>
    );

    clone_raw(flags, stack, parent_tid, tls, child_tid)
}

// TODO: Never returns
syscall!(exit, bindings::SYS_exit, status: c_int => Infallible<u64>);

syscall!(uname, bindings::SYS_uname, name: *mut bindings::new_utsname => Result<()>);

syscall!(ioctl, bindings::SYS_ioctl, fd: c_uint, cmd: c_uint, arg: c_ulong => Result<c_int>);

syscall!(readlink, bindings::SYS_readlink, path: *const u8, buf: *mut u8, bufsiz: c_size_t => Result<c_size_t>);

syscall!(perf_event_open, bindings::SYS_perf_event_open,
    attr: *const bindings::perf_event_attr, pid: pid_t, cpu: c_int, group_fd: c_int, flags: c_ulong => Result<c_int>);

// NOTE: This technically has 3 arguments but the third one is never used in the
// kernel.
syscall!(getcpu, bindings::SYS_getcpu, cpu: *mut c_uint, node: *mut c_uint => Result<()>);

syscall!(getpid, bindings::SYS_getpid => Infallible<pid_t>);
syscall!(getppid, bindings::SYS_getppid => Infallible<pid_t>);
syscall!(gettid, bindings::SYS_gettid => Infallible<pid_t>);

// TODO: Switch last argument to a bindings::rusage
syscall!(wait4, bindings::SYS_wait4, pid: pid_t, wstatus: *mut c_int, options: c_int, ru: *mut c_void => Result<pid_t>);

syscall!(eventfd, bindings::SYS_eventfd, count: c_uint => Result<c_int>);
syscall!(eventfd2, bindings::SYS_eventfd2, count: c_uint, flags: c_uint => Result<c_int>);

syscall!(poll, bindings::SYS_poll, ufds: *mut bindings::pollfd, nfds: c_uint, timeout: c_int => Result<c_uint>);

syscall!(
    arch_prctl_set, bindings::SYS_arch_prctl, code: c_int, addr: c_ulong => Result<()>
);

syscall!(
    arch_prctl_get, bindings::SYS_arch_prctl, code: c_uint, addr: *mut c_ulong => Result<()>
);
