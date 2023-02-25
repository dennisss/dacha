extern crate alloc;
extern crate core;

#[macro_use]
extern crate common;
extern crate sys;

pub mod clock;
pub mod gpio;
mod memory;
pub mod pcm;
pub mod pwm;
pub mod temp;

mod registers {
    #![allow(dead_code, non_snake_case)]
    include!(concat!(env!("OUT_DIR"), "/src/registers.rs"));
}
