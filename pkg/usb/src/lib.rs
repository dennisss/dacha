#[macro_use]
extern crate common;
#[macro_use]
extern crate failure;
extern crate libc;
#[macro_use]
extern crate nix;

pub mod descriptors;
mod descriptor_iter;
mod endpoint;
mod error;
pub mod hid;
mod language;
mod linux;

pub use error::Error;
pub use language::*;
pub use linux::*;
pub use descriptor_iter::Descriptor;