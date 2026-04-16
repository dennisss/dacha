#![feature(
    maybe_uninit_array_assume_init,
    let_chains,
    inherent_associated_types
)]

extern crate alloc;

#[macro_use]
extern crate common;
#[macro_use]
extern crate macros;

pub mod h264;
pub mod mp4;
pub mod mp4_protection;
