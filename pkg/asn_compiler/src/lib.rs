#[macro_use]
extern crate common;
#[macro_use]
extern crate parsing;

mod build;
mod compiler;
mod syntax;
mod tokenizer;

// Exported with usage in the runtime.
pub mod tag;

pub use build::build;
pub use build::build_in_directory;
