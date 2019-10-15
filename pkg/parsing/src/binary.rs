use bytes::Bytes;
use crate::ParseResult;
use crate::bytes::Buf;
use crate::incomplete_error;

pub fn be_u8(mut input: Bytes) -> ParseResult<u8> {
	if input.len() < 1 {
		return Err(incomplete_error());
	}

	let v = input[0];
	input.advance(1);
	Ok((v, input))
}

pub fn be_u16(mut input: Bytes) -> ParseResult<u16> {
	if input.len() < 2 {
		return Err(incomplete_error());
	}

	let v = u16::from_be_bytes(*array_ref![&input, 0, 2]);
	input.advance(2);
	Ok((v, input))
}

pub fn be_u24(mut input: Bytes) -> ParseResult<u32> {
	if input.len() < 3 {
		return Err(incomplete_error());
	}

	let mut buf = [0u8; 4];
	buf[1..4].copy_from_slice(&input[0..3]);

	let v = u32::from_be_bytes(buf);
	input.advance(3);
	Ok((v, input))
}

pub fn u24_to_be_bytes(v: u32) -> [u8; 3] {
	let buf = v.to_be_bytes();
	*array_ref![buf, 1, 3]
}

pub fn be_u32(mut input: Bytes) -> ParseResult<u32> {
	if input.len() < 4 {
		return Err(incomplete_error());
	}

	let v = u32::from_be_bytes(*array_ref![&input, 0, 4]);
	input.advance(4);
	Ok((v, input))
}