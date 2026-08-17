#![feature(inherent_associated_types)]

#[macro_use]
extern crate macros;

mod inst;
pub use inst::*;

mod protocol;
mod config;