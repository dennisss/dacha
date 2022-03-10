#![feature(
    lang_items,
    asm,
    type_alias_impl_trait,
    inherent_associated_types,
    alloc_error_handler,
    generic_associated_types
)]
#![no_std]

#[cfg(feature = "std")]
extern crate std;

#[cfg(feature = "alloc")]
extern crate alloc;

#[macro_use]
extern crate executor;
extern crate peripherals;
#[macro_use]
extern crate common;
extern crate crypto;
#[macro_use]
extern crate macros;
extern crate nordic_proto;

#[cfg(feature = "alloc")]
pub mod allocator;
pub mod clock;
pub mod ecb;
pub mod eeprom;
pub mod entry;
mod events;
pub mod examples;
pub mod gpio;
pub mod log;
pub mod pins;
pub mod protocol;
pub mod radio;
pub mod radio_socket;
pub mod rng;
#[cfg(feature = "alloc")]
pub mod storage;
pub mod temp;
pub mod timer;
pub mod twim;
pub mod uarte;
pub mod usb;
