#![no_std]
// #[cfg(feature = "std")]
// extern crate std;
#![allow(dead_code, non_snake_case, non_camel_case_types)]

#[macro_use]
extern crate common;

pub mod nrf52840 {
    #![allow(dead_code, non_snake_case, non_camel_case_types)]

    include!(concat!(env!("OUT_DIR"), "/nrf52840.rs"));
}

pub use nrf52840::*;
