#![feature(inherent_associated_types)]

#[macro_use]
extern crate macros;
#[macro_use]
extern crate regexp_macros;

mod media_source;
pub mod mpd;

pub use media_source::*;
