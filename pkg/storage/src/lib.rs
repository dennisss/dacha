#![feature(maybe_uninit_array_assume_init, maybe_uninit_uninit_array)]
#![no_std]

#[cfg(feature = "std")]
#[macro_use]
extern crate std;

#[cfg(feature = "alloc")]
#[macro_use]
extern crate alloc;

#[macro_use]
extern crate common;
#[macro_use]
extern crate uuid_macros;
extern crate crypto;
#[macro_use]
extern crate macros;
#[macro_use]
extern crate parsing;

pub mod devices;
pub mod erasure;
pub mod partition;
mod proto;
mod volume;

pub const LOGICAL_BLOCK_SIZE: usize = 512;

#[derive(Clone, Debug)]
pub struct LogicalBlockRange {
    pub start_block: u64,
    pub num_blocks: u64,
}
