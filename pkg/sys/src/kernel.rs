// This module defines types which MUST be compatible with the Linux kernel's
// definitions. Some of these do not match libc bindings.
//
// TODO: Move this file to //third_party/

#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

use std::time::Duration;

pub type sigset_t = u64;

// Mirroring 'include/uapi/linux/time_types.h'

#[derive(Clone, Copy, Default, Debug)]
#[repr(C)]
pub struct timespec {
    /// Seconds
    pub tv_sec: u64,

    /// Nanoseconds
    pub tv_nsec: u64,
}

impl From<Duration> for timespec {
    fn from(value: Duration) -> Self {
        Self {
            tv_sec: value.as_secs(),
            tv_nsec: value.subsec_nanos() as u64,
        }
    }
}

// TODO: Which file is thie in

#[derive(Clone, Copy, Default, Debug)]
#[repr(C)]
pub struct timespec64 {
    /// Seconds
    pub tv_sec: i64,

    /// Nanoseconds
    pub tv_nsec: i64,
}

impl From<Duration> for timespec64 {
    fn from(value: Duration) -> Self {
        Self {
            tv_sec: value.as_secs() as i64,
            tv_nsec: value.subsec_nanos() as i64,
        }
    }
}

// Mirroring 'include/uapi/linux/io_uring.h'

pub const IORING_ENTER_EXT_ARG: u32 = 1 << 3;

#[derive(Clone, Copy)]
#[repr(C)]
pub enum io_uring_op {
    IORING_OP_NOP = 0,
    IORING_OP_READV,
    IORING_OP_WRITEV,
    IORING_OP_FSYNC,
    IORING_OP_READ_FIXED,
    IORING_OP_WRITE_FIXED,
    IORING_OP_POLL_ADD,
    IORING_OP_POLL_REMOVE,
    IORING_OP_SYNC_FILE_RANGE,
    IORING_OP_SENDMSG,
    IORING_OP_RECVMSG,
    IORING_OP_TIMEOUT,
    IORING_OP_TIMEOUT_REMOVE,
    IORING_OP_ACCEPT,
    IORING_OP_ASYNC_CANCEL,
    IORING_OP_LINK_TIMEOUT,
    IORING_OP_CONNECT,
    IORING_OP_FALLOCATE,
    IORING_OP_OPENAT,
    IORING_OP_CLOSE,
    IORING_OP_FILES_UPDATE,
    IORING_OP_STATX,
    IORING_OP_READ,
    IORING_OP_WRITE,
    IORING_OP_FADVISE,
    IORING_OP_MADVISE,
    IORING_OP_SEND,
    IORING_OP_RECV,
    IORING_OP_OPENAT2,
    IORING_OP_EPOLL_CTL,
    IORING_OP_SPLICE,
    IORING_OP_PROVIDE_BUFFERS,
    IORING_OP_REMOVE_BUFFERS,
    IORING_OP_TEE,
    IORING_OP_SHUTDOWN,
    IORING_OP_RENAMEAT,
    IORING_OP_UNLINKAT,
    IORING_OP_MKDIRAT,
    IORING_OP_SYMLINKAT,
    IORING_OP_LINKAT,
    IORING_OP_MSG_RING,
    IORING_OP_FSETXATTR,
    IORING_OP_SETXATTR,
    IORING_OP_FGETXATTR,
    IORING_OP_GETXATTR,
    IORING_OP_SOCKET,
    IORING_OP_URING_CMD,
    IORING_OP_SEND_ZC,
    IORING_OP_SENDMSG_ZC,
}

#[derive(Clone, Copy, Default, Debug)]
#[repr(C)]
pub struct io_uring_getevents_arg {
    /// Pointer to a sigset_t value.
    pub sigmask: u64,

    /// size_of(sigset_t)
    pub sigmask_sz: u32,

    /// Should always be zero
    pub pad: u32,

    /// Pointer to a 'timespec' object.
    pub ts: u64,
}
