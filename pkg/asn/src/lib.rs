#![feature(const_fn)]

#[macro_use] extern crate lazy_static;
extern crate chrono;

pub mod tag;
mod t61;
pub mod builtin;
pub mod encoding;
pub mod debug;
pub mod tokenizer;
pub mod syntax;
pub mod compiler;
mod build;

pub use build::build;