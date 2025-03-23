use std::ffi::CString;

use crate::{bindings, c_char, c_int, c_uint, Errno, OpenFileDescriptor};

pub use bindings::{IN_CLOEXEC, IN_MODIFY, IN_MOVED_FROM, IN_MOVED_TO, IN_MOVE_SELF};

pub mod raw {
    use super::*;

    syscall!(
        inotify_init1, bindings::SYS_inotify_init1, flags: c_uint => Result<c_int>
    );

    syscall!(
        inotify_add_watch, bindings::SYS_inotify_add_watch, fd: i32, path: *const c_char, mask: u32 => Result<c_int>
    );
}
