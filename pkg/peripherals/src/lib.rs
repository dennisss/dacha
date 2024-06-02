#![feature(generic_associated_types, type_alias_impl_trait)]
#![no_std]

#[macro_use]
extern crate common;

#[macro_use]
extern crate macros;
#[cfg(feature = "std")]
#[macro_use]
extern crate failure;
#[cfg(feature = "alloc")]
#[macro_use]
extern crate alloc;
#[cfg(feature = "std")]
#[macro_use]
extern crate std;

#[cfg(feature = "std")]
#[macro_use]
extern crate nix;

pub mod raw {
    pub use peripherals_raw::*;
}

pub mod blob;
pub mod eeprom;
pub mod storage;

#[cfg(feature = "std")]
mod linux;
#[cfg(feature = "std")]
pub use linux::*;
