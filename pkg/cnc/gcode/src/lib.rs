#![feature(inherent_associated_types)]

#[macro_use]
extern crate macros as other_macros;

use std::{collections::HashMap, time::Instant};

pub use gcode_decimal::Decimal;

#[macro_use]
extern crate regexp_macros;
#[macro_use]
extern crate gcode_macros;

mod command;
mod hints;
#[macro_use]
mod macros;
mod line;
mod metadata;
mod parser;
mod program;
mod tiling;

use base_error::*;

pub use crate::command::*;
pub use crate::line::*;
pub use crate::metadata::*;
pub use crate::parser::*;
pub use crate::program::*;
pub use crate::tiling::*;

/// TODO: The gRBL limit is only 128.
pub const MAX_STANDARD_LINE_LENGTH: usize = 256;

/// See https://linuxcnc.org/docs/html/gcode/coordinates.html
/// These are the Gcodes for selecting coordinate system 1 to 9 in Linux CNC /
/// gRBL / Smoothieware firmwares.
///
/// TODO: Get rid of this and rely only on STANDARD_COORDINATE_SYSTEM_CODES
pub const STANDARD_COORDINATE_SYSTEMS: &'static [&'static str] = &[
    "G54", "G55", "G56", "G57", "G58", "G59", "G59.1", "G59.2", "G59.3",
];

pub const STANDARD_COORDINATE_SYSTEM_CODES: &'static [CommandWord] = &[
    command_word!("G54"),
    command_word!("G55"),
    command_word!("G56"),
    command_word!("G57"),
    command_word!("G58"),
    command_word!("G59"),
    command_word!("G59.1"),
    command_word!("G59.2"),
    command_word!("G59.3"),
];
