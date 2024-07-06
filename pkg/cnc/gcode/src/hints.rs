// This file data type contains hints for certain special GCode commands which
// can't be parsed normally since they don't conform to standard GCode syntaxes.

use crate::decimal::Decimal;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum WordValueTypeHint {
    Unknown,
    EndTerminatedString,
}

pub type WordValueHintLookup = fn(u8) -> WordValueTypeHint;

pub fn default_gcode_hints(key: u8, value: Decimal) -> Option<WordValueHintLookup> {
    if key == b'M' && value == Decimal::from(486) {
        return Some(|k: u8| match k {
            b'A' => WordValueTypeHint::EndTerminatedString,
            _ => WordValueTypeHint::Unknown,
        });
    }

    None
}
