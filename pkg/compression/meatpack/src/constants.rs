use base_error::*;

pub const ESCAPE_CODE: u8 = 0xff;

pub const LOOKUP_4_TO_8_BIT: [u8; 16] = [
    b'0', b'1', b'2', b'3', b'4', b'5', b'6', b'7', b'8', b'9', b'.', b' ', b'\n', b'G', b'X', 0,
];

pub const LOOKUP_4_TO_8_BIT_NO_SPACES: [u8; 16] = [
    b'0', b'1', b'2', b'3', b'4', b'5', b'6', b'7', b'8', b'9', b'.', b'E', b'\n', b'G', b'X', 0,
];

enum_def!(Command u8 =>
    None = 0,
    EnablePacking = 0xFB,
    DisablePacking = 0xFA,
    ResetAll = 0xF9,
    QueryConfig = 0xF8,
    EnableNoSpaces = 0xF7,
    DisableNoSpaces = 0xF6
);
