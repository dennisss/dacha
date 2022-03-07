#![no_std]

#[macro_use]
extern crate common;

pub mod register;

#[cfg(label = "cortex_m")]
pub mod nrf52840 {
    #![allow(
        dead_code,
        non_snake_case,
        non_camel_case_types,
        unused_imports,
        unused_variables
    )]

    include!(concat!(env!("OUT_DIR"), "/nrf52840.rs"));
}

#[cfg(label = "cortex_m")]
pub use nrf52840::*;

#[cfg(label = "cortex_m")]
pub mod nvic;
