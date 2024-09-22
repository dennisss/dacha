extern crate alloc;
extern crate core;

mod proto {
    #![allow(dead_code, non_snake_case)]
    include!(concat!(env!("OUT_DIR"), "/src/raw.rs"));
}

mod decoder;
mod decoder_raw;
mod params;
mod program;

pub use decoder::*;
pub use decoder_raw::*;
pub use params::*;
pub use program::*;

pub const FILE_MAGIC: &'static [u8] = b"GCDE";
