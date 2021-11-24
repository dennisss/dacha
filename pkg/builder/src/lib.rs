/*
How bundle will work:
- Compile all dependencies.
- Take all absolute_srcs and
- Will be moved into build/pkg/sensor_monitor/bundle.tar
    - Mapped to 'pkg/sensor_monitor/bundle.tar' if used in the future

*/

extern crate common;
#[macro_use]
extern crate macros;
extern crate compression;
extern crate crypto;
extern crate nix;

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
