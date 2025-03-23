use std::ffi::CString;

use crate::{bindings, c_char, c_int, c_uint, Errno, OpenFileDescriptor};

pub use bindings::{
    FAN_CLASS_CONTENT, FAN_CLASS_NOTIF, FAN_CLOEXEC, FAN_MARK_ADD, FAN_MODIFY, FAN_NONBLOCK,
    FAN_REPORT_FID, O_CLOEXEC,
};

pub mod raw {
    use super::*;

    syscall!(
        fanotify_init, bindings::SYS_fanotify_init, flags: c_uint, event_f_flags: c_uint => Result<c_int>
    );

    syscall!(
        fanotify_mark, bindings::SYS_fanotify_mark, fanotify_fd: c_int, flags: c_uint, mask: u64, dirfd: c_int, pathname: *const c_char => Result<()>
    );
}
