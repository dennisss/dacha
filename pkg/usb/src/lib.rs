#[macro_use] extern crate common;
#[macro_use] extern crate failure;
extern crate libc;
#[macro_use] extern crate nix;

pub mod descriptors;
mod linux;
mod endpoint;
mod error;
mod language;
pub mod hid;

pub use error::{Error, ErrorKind};
pub use language::*;
pub use linux::*;
