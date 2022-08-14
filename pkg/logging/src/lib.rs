#![no_std]

#[cfg(feature = "std")]
#[macro_use]
extern crate std;

#[cfg(feature = "alloc")]
#[macro_use]
extern crate alloc;

#[macro_use]
extern crate macros;
#[macro_use]
extern crate common;
extern crate executor;
extern crate protobuf;
pub extern crate nordic_proto;

pub mod logger;

pub use logger::*;


#[macro_export]
macro_rules! assert_no_debug {
    ($v:expr) => {
        $crate::assert_no_debug_impl($v);
    };
}

#[inline(never)]
pub fn assert_no_debug_impl(value: bool) {
    if !value {
        panic!();
    }
}