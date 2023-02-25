use std::ffi::CString;

use crate::{bindings, c_char, c_int, c_ulong, c_void, Errno};

define_bit_flags!(MountFlags c_ulong {
    MS_REMOUNT = (bindings::MS_REMOUNT as c_ulong),
    MS_BIND = (bindings::MS_BIND as c_ulong),
    MS_SHARED = (bindings::MS_SHARED as c_ulong),
    MS_PRIVATE = (bindings::MS_PRIVATE as c_ulong),
    MS_SLAVE = (bindings::MS_SLAVE as c_ulong),
    MS_UNBINDABLE = (bindings::MS_UNBINDABLE as c_ulong),
    MS_MOVE = (bindings::MS_MOVE as c_ulong),
    MS_DIRSYNC = (bindings::MS_DIRSYNC as c_ulong),
    MS_LAZYTIME = (bindings::MS_LAZYTIME as c_ulong),
    MS_MANDLOCK = (bindings::MS_MANDLOCK as c_ulong),
    MS_NOATIME = (bindings::MS_NOATIME as c_ulong),
    MS_NODEV = (bindings::MS_NODEV as c_ulong),
    MS_NODIRATIME = (bindings::MS_NODIRATIME as c_ulong),
    MS_NOEXEC = (bindings::MS_NOEXEC as c_ulong),
    MS_NOSUID = (bindings::MS_NOSUID as c_ulong),
    MS_RDONLY = (bindings::MS_RDONLY as c_ulong),
    MS_REC = (bindings::MS_REC as c_ulong),
    MS_RELATIME = (bindings::MS_RELATIME as c_ulong),
    MS_SILENT = (bindings::MS_SILENT as c_ulong),
    MS_STRICTATIME = (bindings::MS_STRICTATIME as c_ulong),
    MS_SYNCHRONOUS = (bindings::MS_SYNCHRONOUS as c_ulong),
    // MS_NOSYMFOLLOW = (bindings::MS_NOSYMFOLLOW as c_ulong)
});

define_bit_flags!(UmountFlags c_int {
    // MNT_FORCE = (bindings::MNT_FORCE as c_int),
    // MNT_DETACH = (bindings::MNT_DETACH as c_int),
    // MNT_EXPIRE = (bindings::MNT_EXPIRE as c_int),
    // UMOUNT_NOFOLLOW = (bindings::UMOUNT_NOFOLLOW as c_int)
});

pub fn mount(
    dev_name: Option<&str>,
    dir_name: &str,
    typ: Option<&str>,
    flags: MountFlags,
    data: Option<&str>,
) -> Result<(), Errno> {
    let dev_name = dev_name.map(|v| CString::new(v).unwrap());
    let dir_name = CString::new(dir_name).unwrap();
    let typ = typ.map(|v| CString::new(v).unwrap());
    let data = data.map(|v| CString::new(v).unwrap());

    unsafe {
        raw::mount(
            dev_name
                .as_ref()
                .map(|v| v.as_ptr())
                .unwrap_or(core::ptr::null()),
            dir_name.as_ptr(),
            typ.as_ref()
                .map(|v| v.as_ptr())
                .unwrap_or(core::ptr::null()),
            flags.to_raw(),
            data.as_ref()
                .map(|v| v.as_ptr() as *const c_void)
                .unwrap_or(core::ptr::null()),
        )
    }
}

pub fn umount(target: &str, flags: UmountFlags) -> Result<(), Errno> {
    let target = CString::new(target).unwrap();

    unsafe { raw::umount2(target.as_ptr(), flags.to_raw()) }
}

mod raw {
    use super::*;

    syscall!(
        mount, bindings::SYS_mount,
        dev_name: *const c_char,
        dir_name: *const c_char,
        typ: *const c_char,
        flags: c_ulong,
        data: *const c_void => Result<()>
    );

    syscall!(
        umount2,
        bindings::SYS_umount2,
        target: *const c_char,
        flags: c_int => Result<()>
    );
}
