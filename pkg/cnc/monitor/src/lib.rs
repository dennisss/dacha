#[macro_use]
extern crate common;
#[macro_use]
extern crate macros;
#[macro_use]
extern crate regexp_macros;

mod camera_controller;
mod camera_recorder;
mod change;
mod config;
mod db;
mod devices;
mod fake_machine;
mod files;
mod instance;
mod metric;
mod player;
mod presets;
pub mod program;
mod response_parser;
mod serial_controller;
mod serial_receiver_buffer;
mod serial_send_buffer;
mod tables;

pub use instance::MonitorImpl;
