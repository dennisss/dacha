extern crate alloc;

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
mod devices;
mod fake_machine;
mod files;
mod instance;
mod metric;
mod ops;
mod player;
pub mod player_preprocessor;
pub mod presets;
pub mod program;
pub mod program_preview;
mod program_preview_manager;
mod response_parser;
mod serial_controller;
mod serial_receiver_buffer;
mod serial_send_buffer;
pub mod syslog_parser;
mod tables;
mod timestamped_value;

use std::time::Duration;

pub use instance::MonitorImpl;

pub fn round_number(v: f32) -> f32 {
    format!("{:.4}", v).parse().unwrap()
}

pub fn round_number_ref(v: &mut f32) {
    *v = round_number(*v);
}
