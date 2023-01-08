use crate::{bindings, c_int, Errno};

// TODO: Check these.
type uid_t = c_int;
type gid_t = c_int;

#[derive(Clone, Copy, Debug, PartialEq)]
#[repr(transparent)]
pub struct Uid(uid_t);

impl Uid {
    pub fn as_raw(&self) -> uid_t {
        self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
#[repr(transparent)]
pub struct Gid(gid_t);

impl Gid {
    pub fn as_raw(&self) -> uid_t {
        self.0
    }
}

#[derive(Debug)]
pub struct ProcessIds<T> {
    pub real: T,
    pub effective: T,
    pub saved: T,
}

pub fn getresuid() -> Result<ProcessIds<Uid>, Errno> {
    let mut ids = ProcessIds {
        real: Uid(0),
        effective: Uid(0),
        saved: Uid(0),
    };

    unsafe { raw::getresuid(&mut ids.real.0, &mut ids.effective.0, &mut ids.saved.0)? };

    Ok(ids)
}

pub fn getresgid() -> Result<ProcessIds<Gid>, Errno> {
    let mut ids = ProcessIds {
        real: Gid(0),
        effective: Gid(0),
        saved: Gid(0),
    };

    unsafe { raw::getresgid(&mut ids.real.0, &mut ids.effective.0, &mut ids.saved.0)? };

    Ok(ids)
}

pub unsafe fn chown(path: *const u8, uid: Uid, gid: Gid) -> Result<(), Errno> {
    raw::chown(path, uid.0, gid.0)
}

mod raw {
    use super::*;

    syscall!(getresuid, bindings::SYS_getresuid, ruid: *mut uid_t, euid: *mut uid_t, suid: *mut uid_t => Result<()>);
    syscall!(getresgid, bindings::SYS_getresgid, rgid: *mut gid_t, egid: *mut gid_t, sgid: *mut gid_t => Result<()>);

    syscall!(chown, bindings::SYS_chown, path: *const u8, uid: uid_t, gid: gid_t => Result<()>);
}
