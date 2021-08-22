#[macro_use]
extern crate common;
extern crate libc;
#[macro_use]
extern crate nix;
#[macro_use]
extern crate lazy_static;

pub mod ddc;
pub mod i2c;
pub mod spi;

pub mod ds3231;
pub mod flash;
