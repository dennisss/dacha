use common::errors::*;
use protobuf::wire::parse_varint;


pub fn parse_slice(mut input: &[u8]) -> Result<(&[u8], &[u8])> {
	let len = parse_next!(input, parse_varint);
	if input.len() < len {
		return Err("Slice out of range".into());
	}

	Ok(input.split_at(len))
}

pub fn parse_string(mut input: &[u8]) -> Result<(String, &[u8])> {
	let data = parse_next!(input, parse_slice);
	Ok((String::from_utf8(data.to_vec())?, input))
}

pub fn parse_fixed32(mut input: &[u8]) -> Result<(u32, &[u8])> {
	if input.len() < 4 {
		return Err("Input too short for fixed32".into());
	}

	let val = u32::from_le_bytes(*array_ref![input, 0, 4]);
	Ok((val, &input[4..]))
}

pub fn parse_fixed64(mut input: &[u8]) -> Result<(u64, &[u8])> {
	if input.len() < 8 {
		return Err("Input too short for fixed64".into());
	}

	let val = u64::from_le_bytes(*array_ref![input, 0, 8]);
	Ok((val, &input[8..]))
}

pub fn parse_u8(mut input: &[u8]) -> Result<(u8, &[u8])> {
	if input.len() < 1 {
		return Err("Input too short for u8".into());
	}

	Ok((input[0], &input[1..]))
}

// TODO: This assumes a native little-endian system.
// - we should swap the bytes in place if on a big-endian system
pub fn u32_slice(input: &[u8]) -> &[u32] {
	unsafe {
		std::slice::from_raw_parts(input.as_ptr() as *const u32,
								   input.len() / 4)
	}
}