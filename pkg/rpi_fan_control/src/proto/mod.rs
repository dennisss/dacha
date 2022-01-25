#![allow(dead_code, non_snake_case)]

pub mod rpi_fan_control {
    include!(concat!(env!("OUT_DIR"), "/src/proto/rpi_fan_control.rs"));
}
