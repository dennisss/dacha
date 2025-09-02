#![feature(inherent_associated_types)]

#[macro_use]
extern crate common;
#[macro_use]
extern crate macros;

mod web_client;
mod utils;

pub use web_client::*;
pub use utils::*;
