#![no_std]

extern crate peripherals_raw;

pub mod raw {
    pub use peripherals_raw::*;
}

/*
Could do something like:
RUSTFLAGS='--cfg target_model="nrf52840"'
*/
