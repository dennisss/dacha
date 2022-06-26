#![feature(c_size_t)]

#[macro_use]
extern crate common;
#[macro_use]
extern crate regexp_macros;

mod errno;
#[macro_use]
mod syscall_amd64;

pub mod virtual_memory;

pub mod bindings {
    #![allow(non_upper_case_globals)]
    #![allow(non_camel_case_types)]
    #![allow(non_snake_case)]

    include!(concat!(env!("OUT_DIR"), "/bindings.rs"));
}


pub use errno::*;
pub use core::ffi::c_size_t;
pub use std::os::raw::{c_int, c_ushort, c_uint, c_char, c_ulong};
pub use virtual_memory::*;

pub type umode_t = c_ushort;

// TODO: Check this.
pub type off_t = c_size_t;

// TODO: Check this.
// This should be 32bit.
pub type pid_t = c_int;

// from fcntl.h
pub const O_RDONLY: c_int = 0x0000;

pub const O_CLOEXEC: c_int = 0x80000;

pub const SEEK_SET: c_uint = 0;


type ZERO = ();
type Addr = *mut u8;

syscall_amd64!(read, 0x00, fd: c_int, buf: *mut u8, count: c_size_t => c_size_t);
syscall_amd64!(write, 0x01, fd: c_int, buf: *const u8, count: c_size_t => c_size_t);
syscall_amd64!(open, 0x02, path: *const c_char, flags: c_int, mode: umode_t => c_int);
syscall_amd64!(close, 0x03, fd: c_int => ZERO);
syscall_amd64!(lseek, 0x08, fd: c_int, offset: off_t, whence: c_uint => off_t);
syscall_amd64!(mmap, 0x09, addr: *mut u8, length: c_size_t, prot: c_int, flags: c_int, fd: c_int, offset: off_t => Addr);

syscall_amd64!(ioctl, 0x10, fd: c_uint, cmd: c_uint, arg: c_ulong => c_int);

syscall_amd64!(perf_event_open, 0x12a,
    attr: *const bindings::perf_event_attr, pid: pid_t, cpu: c_int, group_fd: c_int, flags: c_ulong => c_int);

// TODO: Some syscalls like getpid() and getppid() always succeed so we don't need them to return a Result<>.