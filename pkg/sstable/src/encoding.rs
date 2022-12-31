use common::errors::*;
use protobuf::wire::{parse_varint, serialize_varint};

pub fn parse_slice(mut input: &[u8]) -> Result<(&[u8], &[u8])> {
    let len = parse_next!(input, parse_varint) as usize;
    if input.len() < len {
        return Err(err_msg("Slice out of range"));
    }

    Ok(input.split_at(len))
}

pub fn serialize_slice(data: &[u8], out: &mut Vec<u8>) {
    serialize_varint(data.len() as u64, out);
    out.extend_from_slice(data);
}

pub fn parse_string(mut input: &[u8]) -> Result<(String, &[u8])> {
    let data = parse_next!(input, parse_slice);
    Ok((String::from_utf8(data.to_vec())?, input))
}

pub fn serialize_string(value: &str, out: &mut Vec<u8>) {
    serialize_slice(value.as_bytes(), out);
}

pub fn parse_fixed32(input: &[u8]) -> Result<(u32, &[u8])> {
    if input.len() < 4 {
        return Err(err_msg("Input too short for fixed32"));
    }

    let val = u32::from_le_bytes(*array_ref![input, 0, 4]);
    Ok((val, &input[4..]))
}

pub fn parse_fixed64(input: &[u8]) -> Result<(u64, &[u8])> {
    if input.len() < 8 {
        return Err(err_msg("Input too short for fixed64"));
    }

    let val = u64::from_le_bytes(*array_ref![input, 0, 8]);
    Ok((val, &input[8..]))
}

pub fn parse_u8(input: &[u8]) -> Result<(u8, &[u8])> {
    if input.len() < 1 {
        return Err(err_msg("Input too short for u8"));
    }

    Ok((input[0], &input[1..]))
}

// TODO: This assumes a native little-endian system.
// - we should swap the bytes in place if on a big-endian system
pub fn u32_slice(input: &[u8]) -> &[u32] {
    unsafe { std::slice::from_raw_parts(input.as_ptr() as *const u32, input.len() / 4) }
}
