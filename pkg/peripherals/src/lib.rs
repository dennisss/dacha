#![feature(generic_associated_types, type_alias_impl_trait)]
#![no_std]

#[macro_use]
extern crate common;
extern crate crypto;
extern crate executor;
extern crate peripherals_raw;

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

pub mod raw {
    pub use peripherals_raw::*;
}

pub mod blob;
pub mod eeprom;
pub mod storage;

/*
Could do something like:
RUSTFLAGS='--cfg target_model="nrf52840"'
*/
