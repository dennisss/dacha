use alloc::string::{String, ToString};

use base_error::*;

pub fn read_null_terminated_str(data: &[u8]) -> Result<&str> {
    for i in 0..data.len() {
        if data[i] == 0x00 {
            return Ok(core::str::from_utf8(&data[0..i])?);
        }
    }

    Err(err_msg("Missing null terminator"))
}

pub trait ByteType {}

impl ByteType for u8 {}
impl ByteType for i8 {}

pub fn read_null_terminated_string<T: ByteType>(data: &[T]) -> Result<String> {
    let data = unsafe { core::mem::transmute(data) };

    read_null_terminated_str(data).map(|s| s.to_string())
}
