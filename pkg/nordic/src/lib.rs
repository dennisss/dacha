#![feature(
    lang_items,
    type_alias_impl_trait,
    impl_trait_in_assoc_type,
    inherent_associated_types,
    alloc_error_handler,
    generic_associated_types,
    trait_alias,
    core_intrinsics
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
// pub mod config_storage;
pub mod ecb;
pub mod eeprom;
pub mod entry;
mod events;
pub mod examples;
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
pub mod controller;
mod fpu;
pub mod keyboard;
pub mod params;
pub mod pwm;
pub mod reset;
pub mod rtc;
pub mod temp;
pub mod tmc2130;
pub mod twim;
pub mod uarte;
pub mod usb;
pub mod timer;
pub mod gpiote;
pub mod ppi;
pub mod adc;
pub mod neopixel;
pub mod idle;
pub mod ram;
pub mod sensor;

pub use fpu::*;
