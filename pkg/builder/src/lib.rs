extern crate alloc;
extern crate core;

extern crate common;
#[macro_use]
extern crate macros;
extern crate compression;
extern crate crypto;

mod builder;
pub mod cli;
mod context;
mod label;
mod platform;
pub mod proto;
mod target;

pub use builder::{BuildResult, BuildResultKey, Builder};
pub use context::BuildContext;
pub use platform::current_platform;

pub const LOCAL_BINARY_PATH: &'static str = "bin";
