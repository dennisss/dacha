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
#[cfg(feature = "alloc")]
#[macro_use]
extern crate failure;
#[cfg(all(feature = "std", target_os = "linux"))]
extern crate libc;
#[cfg(all(feature = "std", target_os = "linux"))]
#[macro_use]
extern crate nix;

#[cfg(feature = "alloc")]
pub mod descriptor_builders;
#[cfg(feature = "alloc")]
pub mod descriptor_iter; // TOOD: Make private?
pub mod descriptor_set;
pub mod descriptors;
pub mod dfu;
mod endpoint;
mod error;
#[cfg(feature = "alloc")]
pub mod hid;
#[cfg(feature = "alloc")]
mod language;
#[cfg(all(feature = "std", target_os = "linux"))]
mod linux;
#[cfg(all(feature = "std", target_os = "linux"))]
mod local_string;

#[cfg(feature = "alloc")]
pub use descriptor_iter::Descriptor;
pub use descriptor_set::DescriptorSet;
pub use error::Error;
#[cfg(feature = "alloc")]
pub use language::*;
#[cfg(all(feature = "std", target_os = "linux"))]
pub use linux::*;
