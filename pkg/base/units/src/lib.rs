#![no_std]

#[cfg(feature = "std")]
#[macro_use]
extern crate std;

#[cfg(feature = "alloc")]
extern crate alloc;

mod byte;

pub use byte::*;
