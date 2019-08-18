// Shared utilities between the inflate and deflate implementations. 

use common::errors::*;
use crate::huffman::*;
use crate::bits::*;

pub const INFLATE_EARLY_END: &'static str = "End of stream before final block";

pub const END_OF_BLOCK: usize = 256;

/// The code length huffman tree that encodes code lengths is serialized as an array of code lengths where the code lengths for specific symbols are written out in this order.
pub const CODE_LEN_CODE_LEN_ORDERING: [u8; 19] = [
	16, 17, 18, 0, 8, 7, 9, 6, 10, 5,
	11, 4, 12, 3, 13, 2, 14, 1, 15];


// Fixed tree for literal/length alphabet to be used if no dynamic tree is specified.
pub fn fixed_huffman_lenlit_tree() -> Result<HuffmanTree> {
	let mut lens = vec![];
	for i in 0..144 {
		lens.push(8);
	}
	for i in 144..256 {
		lens.push(9);
	}
	for i in 256..280 {
		lens.push(7);
	}
	for i in 280..288 {
		lens.push(8);
	}

	HuffmanTree::from_canonical_lens(&lens)
}

// Fixed 
pub fn fixed_huffman_dist_tree() -> Result<HuffmanTree> {
	let mut lens = vec![];
	lens.resize(32, 5);

	HuffmanTree::from_canonical_lens(&lens)
}



// TODO: Will also need an encoding version
pub fn read_len(code: usize, strm: &mut BitStream) -> Result<usize> {
	Ok(match code {
		257...264 => (code - 257 + 3),
		265...268 => {
			let b = strm.read_bits_exact(1)?;
			2*(code - 265) + b + 11
		},
		269...272 => {
			let b = strm.read_bits_exact(2)?;
			4*(code - 269) + b + 19
		},
		273...276 => {
			let b = strm.read_bits_exact(3)?;
			8*(code - 273) + b + 35
		},
		277...280 => {
			let b = strm.read_bits_exact(4)?;
			16*(code - 277) + b + 67
		},
		281...284 => {
			let b = strm.read_bits_exact(5)?;
			32*(code - 281) + b + 131
		},
		285 => 258,
		_ => { return Err("Invalid length code".into()); }
	})
}

// pub fn write_len(len: usize, litlen_codes: &[BitVector]) -> Result<()> {

// }

pub fn read_distance(code: usize, strm: &mut BitStream) -> Result<usize> {
	if code <= 3 {
		Ok(code + 1)
	} else if code <= 29 {
		let nbits = ((code - 4) / 2) + 1;
		let mul = 1 << nbits;
		let start = (mul << 1) + 1;
		let b = strm.read_bits_exact(nbits as u8)?;

		Ok(mul*(code % 2) + start + b)
	} else {
		Err("Invalid distance code".into())
	}
}

// pub fn write_distance(dist: usize, dist_codes: &[BitVector]) -> Result<()> {
// 	if dist < 1 || dist > 32768 {
// 		return Err("Distance out of allowed range".into());
// 	}

// 	if dist <= 4 {
// 		let code = dist + 1;
// 		// No extra bits
// 	} else {
// 		let nbits = ((dist as f32).log2().floor() - 1) as u8;
// 		let mul = 1 << nbits;
// 		let start = (mul << 1) + 1;

// 		let code = 2*(nbits - 1) + 4;
// 		let extra = dist - start;
// 		// Now we should write both
// 	}

// 	Ok(())
// }


#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn read_len_test() {
		let try_with = |code, extra| {
			let data = vec![extra];
			let mut c = std::io::Cursor::new(data);
			let mut strm = BitStream::new(&mut c);
			read_len(code, &mut strm).unwrap()
		};

		assert_eq!(try_with(257, 0), 3);
		assert_eq!(try_with(266, 0b1), 14);
		assert_eq!(try_with(270, 0b10), 25);
	}

	#[test]
	fn read_distance_test() {
		let try_with = |code, extra, extra2| {
			let data = vec![extra, extra2];
			let mut c = std::io::Cursor::new(data);
			let mut strm = BitStream::new(&mut c);
			read_distance(code, &mut strm).unwrap()
		};

		assert_eq!(try_with(2, 0, 0), 3);
		assert_eq!(try_with(8, 0b011, 0), 20);
		assert_eq!(try_with(9, 0b010, 0), 27);
	}
}