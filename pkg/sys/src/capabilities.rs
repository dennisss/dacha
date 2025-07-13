// Utility for configuring linux capabilities and securebits
//
// Secure bits are defined in:
// https://github.com/torvalds/linux/blob/5bfc75d92efd494db37f5c4c173d3639d4772966/include/uapi/linux/securebits.h
//
// Capability syscalls are defined here:
// https://github.com/torvalds/linux/blob/master/include/uapi/linux/capability.h#L36

use crate::{bindings, c_int, pid_t, Errno};

const LINUX_CAPABILITY_VERSION_3: u32 = 0x20080522;

#[derive(Clone, Debug)]
pub struct CapabilitiesData {
    pub effective: CapabilitiesSet,
    pub permitted: CapabilitiesSet,
    pub inheritable: CapabilitiesSet,
}

define_bit_flags!(CapabilitiesSet u64 {
    CAP_CHOWN = (1 << bindings::CAP_CHOWN),
    CAP_SETGID = (1 << bindings::CAP_SETGID),
    CAP_SETUID = (1 << bindings::CAP_SETUID),
    CAP_SETPCAP = (1 << bindings::CAP_SETPCAP),
    CAP_NET_ADMIN = (1 << bindings::CAP_NET_ADMIN),
    CAP_NET_BIND_SERVICE = (1 << bindings::CAP_NET_BIND_SERVICE),
    CAP_SYS_TIME = (1 << bindings::CAP_SYS_TIME)
});


#[repr(C)]
struct cap_user_header {
    pub version: u32,
    pub pid: pid_t,
}

#[repr(C)]
#[derive(Default, Clone, Copy)]
struct cap_user_data {
    pub effective: u32,
    pub permitted: u32,
    pub inheritable: u32,
}

pub const SECBIT_NOROOT: u32 = 1 << 0;
pub const SECBIT_NOROOT_LOCKED: u32 = 1 << 1;

pub const SECBIT_NO_SETUID_FIXUP: u32 = 1 << 2;
pub const SECBIT_NO_SETUID_FIXUP_LOCKED: u32 = 1 << 3;

pub const SECBIT_KEEP_CAPS: u32 = 1 << 4;
pub const SECBIT_KEEP_CAPS_LOCKED: u32 = 1 << 5;

pub const SECBIT_NO_CAP_AMBIENT_RAISE: u32 = 1 << 6;
pub const SECBIT_NO_CAP_AMBIENT_RAISE_LOCKED: u32 = 1 << 7;

/// Secure bits which prevent a process and all its descendants from gaining
/// capabilities unless executing a program with file capabilities.
pub const SECBITS_LOCKED_DOWN: u32 = SECBIT_NOROOT
    | SECBIT_NOROOT_LOCKED
    | SECBIT_NO_SETUID_FIXUP
    | SECBIT_NO_SETUID_FIXUP_LOCKED
    | SECBIT_KEEP_CAPS_LOCKED
    | SECBIT_NO_CAP_AMBIENT_RAISE
    | SECBIT_NO_CAP_AMBIENT_RAISE_LOCKED;

pub fn capget(pid: pid_t) -> Result<CapabilitiesData, Errno> {
    let hdr = cap_user_header {
        version: LINUX_CAPABILITY_VERSION_3,
        pid,
    };

    let mut data = [cap_user_data::default(); 2];
    unsafe { raw::capget(&hdr, data.as_mut_ptr()) }?;

    Ok(CapabilitiesData {
        effective: ((data[0].effective as u64) | ((data[1].effective as u64) << 32)).into(),
        permitted: ((data[0].permitted as u64) | ((data[1].permitted as u64) << 32)).into(),
        inheritable: ((data[0].inheritable as u64) | ((data[1].inheritable as u64) << 32)).into(),
    })
}

/// NOTE: This is always 2 elements in V3 of the capabilities API. On 64-bit
/// devices, both are used to support 64-bit capability sets.
pub fn capset(pid: pid_t, data: &CapabilitiesData) -> Result<(), Errno> {
    let hdr = cap_user_header {
        version: LINUX_CAPABILITY_VERSION_3,
        pid,
    };

    let mut raw_data = [cap_user_data::default(); 2];
    raw_data[0].effective = data.effective.to_raw() as u32;
    raw_data[1].effective = (data.effective.to_raw() >> 32) as u32;

    raw_data[0].permitted = data.permitted.to_raw() as u32;
    raw_data[1].permitted = (data.permitted.to_raw() >> 32) as u32;

    raw_data[0].inheritable = data.inheritable.to_raw() as u32;
    raw_data[1].inheritable = (data.inheritable.to_raw() >> 32) as u32;

    unsafe { raw::capset(&hdr, raw_data.as_ptr()) }
}

mod raw {
    use super::*;

    syscall!(capget, bindings::SYS_capget, hdrp: *const cap_user_header, datap: *mut cap_user_data => Result<()>);
    syscall!(capset, bindings::SYS_capset, hdrp: *const cap_user_header, datap: *const cap_user_data => Result<()>);
}
