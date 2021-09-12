#[macro_use]
extern crate common;
extern crate libc;
#[macro_use]
extern crate nix;
#[macro_use]
extern crate lazy_static;
#[macro_use]
extern crate parsing;

pub mod ddc;
pub mod i2c;
pub mod spi;

pub mod bmp388;
pub mod ds3231;
pub mod flash;
pub mod sgp30;
