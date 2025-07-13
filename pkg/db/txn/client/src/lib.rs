#[macro_use]
extern crate common;

mod client;
mod remove_overlaps;
// mod iterator;

// TODO: These two modules should only be shared between the client and main
// crate.
pub mod constants;

pub use client::*;
