extern crate common;
extern crate libc;
#[macro_use] extern crate nix;
#[macro_use] extern crate arrayref;

pub mod descriptors;
mod linux;
mod endpoint;

pub use linux::*;