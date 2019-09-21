use bytes::Bytes;
use crate::ParseResult;
use crate::bytes::Buf;

pub fn be_u8(mut input: Bytes) -> ParseResult<u8> {
	if input.len() < 1 {
		return Err("Not enough bytes".into());
	}

	let v = input[0];
	input.advance(1);
	Ok((v, input))
}

pub fn be_u16(mut input: Bytes) -> ParseResult<u16> {
	if input.len() < 2 {
		return Err("Not enough bytes".into());
	}

	let v = u16::from_be_bytes(*array_ref![&input, 0, 2]);
	input.advance(2);
	Ok((v, input))
}

pub fn be_u32(mut input: Bytes) -> ParseResult<u32> {
	if input.len() < 4 {
		return Err("Not enough bytes".into());
	}

	let v = u32::from_be_bytes(*array_ref![&input, 0, 4]);
	input.advance(4);
	Ok((v, input))
}