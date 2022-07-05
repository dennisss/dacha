#![feature(c_size_t, cstr_from_bytes_until_nul)]

#[macro_use]
extern crate common;
#[macro_use]
extern crate regexp_macros;

mod errno;
#[macro_use]
mod syscall_amd64;
mod mapped_memory;
mod num_cpus;

pub mod utsname;
pub mod virtual_memory;

pub mod bindings {
    #![allow(non_upper_case_globals)]
    #![allow(non_camel_case_types)]
    #![allow(non_snake_case)]

    include!(concat!(env!("OUT_DIR"), "/bindings.rs"));
}

pub use core::ffi::c_size_t;
pub use errno::*;
pub use mapped_memory::*;
pub use num_cpus::*;
pub use std::os::raw::{c_char, c_int, c_uint, c_ulong, c_ushort};
pub use virtual_memory::*;

pub type umode_t = c_ushort;

// TODO: Check this.
pub type off_t = c_size_t;

// TODO: Check this.
// This should be 32bit.
pub type pid_t = c_int;

pub const SEEK_SET: c_uint = 0;

pub use bindings::{O_CLOEXEC, O_RDONLY};

/*
See also nice list of syscalls here:
https://chromium.googlesource.com/chromiumos/docs/+/master/constants/syscalls.md
*/

syscall_amd64!(read, bindings::SYS_read, fd: c_int, buf: *mut u8, count: c_size_t => Result<c_size_t>);
syscall_amd64!(write, bindings::SYS_write, fd: c_int, buf: *const u8, count: c_size_t => Result<c_size_t>);
syscall_amd64!(open, bindings::SYS_open, path: *const c_char, flags: c_uint, mode: umode_t => Result<c_int>);
syscall_amd64!(close, bindings::SYS_close, fd: c_int => Result<()>);
syscall_amd64!(lseek, bindings::SYS_lseek, fd: c_int, offset: off_t, whence: c_uint => Result<off_t>);
syscall_amd64!(mmap, bindings::SYS_mmap, addr: *mut u8, length: c_size_t, prot: c_uint, flags: c_uint, fd: c_int, offset: off_t => Result<*mut u8>);
syscall_amd64!(munmap, bindings::SYS_munmap, addr: *mut u8, length: c_size_t => Result<()>);

syscall_amd64!(uname, bindings::SYS_uname, name: *mut bindings::new_utsname => Result<()>);

syscall_amd64!(ioctl, bindings::SYS_ioctl, fd: c_uint, cmd: c_uint, arg: c_ulong => Result<c_int>);

syscall_amd64!(perf_event_open, bindings::SYS_perf_event_open,
    attr: *const bindings::perf_event_attr, pid: pid_t, cpu: c_int, group_fd: c_int, flags: c_ulong => Result<c_int>);

// NOTE: This technically has 3 arguments but the third one is never used in the
// kernel.
syscall_amd64!(getcpu, bindings::SYS_getcpu, cpu: *mut c_uint, node: *mut c_uint => Result<()>);

syscall_amd64!(getpid, bindings::SYS_getpid => Infallible<pid_t>);
syscall_amd64!(getppid, bindings::SYS_getppid => Infallible<pid_t>);
syscall_amd64!(gettid, bindings::SYS_gettid => Infallible<pid_t>);

// TODO: Some syscalls like getpid() and getppid() always succeed so we don't
// need them to return a Result<>.
