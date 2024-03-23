#![no_std]

#[macro_use]
extern crate std;

#[macro_use]
extern crate alloc;

mod descriptor_pool;
mod message;
mod spec;
mod syntax;

pub use descriptor_pool::*;
pub use message::*;
pub use spec::Syntax;