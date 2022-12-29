#![feature(trait_alias)]
#![no_std]

#[cfg(feature = "std")]
#[macro_use]
extern crate std;

#[cfg(feature = "alloc")]
#[macro_use]
extern crate alloc;

#[cfg(feature = "std")]
#[macro_use]
pub extern crate failure;

#[cfg(feature = "std")]
mod error_failure;
#[cfg(feature = "std")]
pub use error_failure::*;

pub mod error_new;
#[cfg(not(feature = "std"))]
pub use error_new::*;
