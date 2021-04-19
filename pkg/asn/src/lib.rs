#![feature(const_fn)]

#[macro_use]
extern crate lazy_static;

mod build;
pub mod builtin;
pub mod compiler;
pub mod debug;
pub mod encoding;
pub mod syntax;
mod t61;
pub mod tag;
pub mod tokenizer;

pub use build::build;
