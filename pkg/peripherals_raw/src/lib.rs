#![no_std]

#[macro_use]
extern crate common;

pub mod register;

#[cfg(target_label = "nrf52840")]
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

#[cfg(target_label = "nrf52840")]
pub use nrf52840::*;

#[cfg(target_label = "cortex_m")]
pub mod nvic;
