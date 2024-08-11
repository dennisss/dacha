// This module contains utilities to create and read dev_t ids.

pub use crate::bindings::dev_t;
use crate::c_uint;

pub fn makedev(major: c_uint, minor: c_uint) -> dev_t {
    let major = major as u64;
    let minor = minor as u64;

    (minor & 0xff) | ((major & 0xfff) << 8) | ((minor & !0xff) << 12) | ((major & !0xfff) << 32)
}
