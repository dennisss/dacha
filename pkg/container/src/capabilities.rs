// Utility for configuring linux capabilities and securebits
//
// Secure bits are defined in:
// https://github.com/torvalds/linux/blob/5bfc75d92efd494db37f5c4c173d3639d4772966/include/uapi/linux/securebits.h
//
// Capability syscalls are defined here:
// https://github.com/torvalds/linux/blob/master/include/uapi/linux/capability.h#L36

pub const LINUX_CAPABILITY_VERSION_3: u32 = 0x20080522;

#[repr(C)]
pub struct cap_user_header {
    pub version: u32,
    pub pid: libc::c_int,
}

#[repr(C)]
pub struct cap_user_data {
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

pub const SECBITS_LOCKED_DOWN: u32 = SECBIT_NOROOT
    | SECBIT_NOROOT_LOCKED
    | SECBIT_NO_SETUID_FIXUP
    | SECBIT_NO_SETUID_FIXUP_LOCKED
    | SECBIT_KEEP_CAPS_LOCKED
    | SECBIT_NO_CAP_AMBIENT_RAISE
    | SECBIT_NO_CAP_AMBIENT_RAISE_LOCKED;
