#![feature(
    lang_items,
    type_alias_impl_trait,
    inherent_associated_types,
    alloc_error_handler,
    generic_associated_types,
    trait_alias
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
#[macro_use]
extern crate logging;

#[cfg(feature = "alloc")]
pub mod allocator;
pub mod clock;
pub mod config_storage;
pub mod ecb;
pub mod eeprom;
pub mod entry;
mod events;
pub mod examples;
// pub mod fast_timer;
pub mod gpio;
pub mod pins;
pub mod protocol;
pub mod radio;
pub mod radio_activity_led;
pub mod radio_socket;
pub mod rng;
pub mod spi;
// pub mod stepper_motor_controller;
pub mod bootloader;
pub mod keyboard;
pub mod params;
pub mod reset;
pub mod temp;
pub mod timer;
pub mod tmc2130;
pub mod twim;
pub mod uarte;
pub mod usb;
