#![no_std]

#[macro_use]
extern crate std;

#[macro_use]
extern crate alloc;

#[cfg(feature = "descriptors")]
mod descriptor_pool;
#[cfg(feature = "descriptors")]
mod message;
pub mod spec;
pub mod syntax;

#[cfg(feature = "descriptors")]
pub use descriptor_pool::*;
#[cfg(feature = "descriptors")]
pub use message::*;
