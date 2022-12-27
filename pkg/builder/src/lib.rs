extern crate alloc;
extern crate core;

#[macro_use]
extern crate common;
#[macro_use]
extern crate macros;
extern crate compression;
extern crate crypto;
#[macro_use]
extern crate file;

mod builder;
pub mod cli;
mod context;
mod label;
mod package;
mod platform;
pub mod proto;
pub mod rule;
mod rules;
pub mod target;
mod utils;

pub use builder::Builder;
pub use context::BuildConfigTarget;
pub use platform::current_platform;

pub const LOCAL_BINARY_PATH: &'static str = "bin";

/// Label of the rule which produces appropriate settings for the current
/// machine.
pub const NATIVE_CONFIG_LABEL: &'static str = "//pkg/builder/config:native";
