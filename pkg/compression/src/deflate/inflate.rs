
use std::io::Read;
use common::errors::*;
use crate::bits::*;
use crate::huffman::*;
use byteorder::{LittleEndian, ReadBytesExt};
use super::shared::*;

// Will need a 

// TODO: If the lens list ends in 0's then we don't really need to encode it
/// Given the encoded code lengths for the dynamic length/literal and distance code trees, this will encode/compress them into the code length alphabet and write them to the output stream
fn write_dynamic_lens(lens: &[usize]) -> Result<()> {

	// We can only encode code lengths up to 15.
	for len in lens {
		if *len > 15 {
			return Err("Length is too long".into());
		}
	}

	for i in 0..lens.len() {
		let v = lens[i];


		// Look for a sequence of zeros.
		if v == 0 {
			let mut j = i + 1;
			while j < lens.len() && lens[j] == 0 {
				j += 1;
			}

			// TODO: Only forth it if we have more than >= 3 zeros

		}

		// Otherwise, just encode as a plain length

	}

	Ok(())
}

fn read_dynamic_lens(
	strm: &mut BitStream, code_len_tree: &HuffmanTree, nsymbols: usize) -> Result<Vec<usize>> {

	let mut lens = vec![]; // TODO: Reserve elements.
	while lens.len() < nsymbols {
		let c = code_len_tree.read_code(strm)?;

		match c {
			0...15 => {
				lens.push(c);
			},
			16 => {
				let n = 3 + (strm.read_bits(2)?.unwrap());
				let l = *lens.last().unwrap();
				for i in 0..n {
					lens.push(l);
				}
			},
			17 => {
				let n = 3 + (strm.read_bits(3)?.unwrap());
				assert!(n <= 10);
				for i in 0..n {
					lens.push(0);
				}
			},
			18 => {
				let n = 11 + (strm.read_bits(7)?.unwrap());
				assert!(n <= 138);
				for i in 0..n {
					lens.push(0);
				}
			},
			_ => {
				return Err(format!("Invalid code len code {}", c).into())
			}
		}
	}

	// This may not necessarily be true if repetition caused an overflow
	assert_eq!(nsymbols, lens.len());

	Ok(lens)
}

fn read_block_codes(strm: &mut BitStream, litlen_tree: &HuffmanTree,
					dist_tree: &HuffmanTree, out: &mut Vec<u8>) -> Result<()> {
	loop {
		let code = litlen_tree.read_code(strm)?;

		if code < END_OF_BLOCK {
			out.push(code as u8);
		} else if code == END_OF_BLOCK {
			break;
		} else {
			let len = read_len(code, strm)?;
			let dist_code = dist_tree.read_code(strm)?;
			let dist = read_distance(dist_code, strm)?;

			// TODO: Validate in range

			// TODO: Implement faster copy
			let cur = out.len();
			for i in 0..len {
				out.push(out[cur - dist + i]);
			}
		}

		if code == END_OF_BLOCK {
			break;
		}
	}

	Ok(())
}

pub fn read_inflate(reader: &mut dyn Read) -> Result<Vec<u8>> {
	let mut out = vec![];

	let mut strm = BitStream::new(reader);

	// Consume all blocks
	loop {
		let bfinal = strm.read_bits(1)?.ok_or(Error::from(INFLATE_EARLY_END))?;
		let btype = strm.read_bits(2)?.ok_or(Error::from(INFLATE_EARLY_END))?;

		match btype {
			// No compression
			0b00 => {
				strm.align_to_byte();
				let len = strm.read_u16::<LittleEndian>()?;
				let nlen = strm.read_u16::<LittleEndian>()?;
				if len != !nlen {
					return Err("Lengths do not match".into());
				}
				
				let i = out.len();
				out.resize(i + (len as usize), 0);
				strm.read_exact(&mut out[i..])?;
			},
			// Compressed with fixed Huffman codes
			0b01 => {
				let litlen_tree = fixed_huffman_lenlit_tree()?;
				let dist_tree = fixed_huffman_dist_tree()?;

				read_block_codes(&mut strm, &litlen_tree, &dist_tree, &mut out)?;
			},
			// Compressed with dynamic Huffman codes
			0b10 => {

				// TODO: Validate the maximum values for these.

				// Number of literal/length codes - 4.
				let hlit = (strm.read_bits(5)?.ok_or(Error::from(INFLATE_EARLY_END))? as usize) + 257;
				// Number of distance codes - 1.
				let hdist = (strm.read_bits(5)?.ok_or(Error::from(INFLATE_EARLY_END))? as usize) + 1;
				// Number of code length codes - 4
				let hclen = (strm.read_bits(4)?.ok_or(Error::from(INFLATE_EARLY_END))? as usize) + 4;

				// TODO: These can only be u8's?
				let mut code_len_code_lens = [0usize; 19];

				for i in 0..hclen {
					let l = strm.read_bits_exact(3)?;
					code_len_code_lens[CODE_LEN_CODE_LEN_ORDERING[i] as usize]
						= l;
				}

				/*
				TODO:
				If only one distance
				code is used, it is encoded using one bit, not zero bits; in
				this case there is a single code length of one, with one unused
				code.  One distance code of zero bits means that there are no
				distance codes used at all (the data is all literals
				*/

				let code_len_tree = HuffmanTree::from_canonical_lens(
					&code_len_code_lens)?;

				let all_lens = read_dynamic_lens(
					&mut strm, &code_len_tree, hlit + hdist)?;

				let litlen_tree = HuffmanTree::from_canonical_lens(
					&all_lens[0..hlit])?;

				let dist_tree = HuffmanTree::from_canonical_lens(
					&all_lens[hlit..])?;

				read_block_codes(&mut strm, &litlen_tree, &dist_tree, &mut out)?;
			},
			_ => {
				return Err(format!("Invalid BTYPE {}", btype).into());
			}
		}

		if bfinal != 0 {
			break;
		}
	}
	
	println!("{}", String::from_utf8(out.clone()).unwrap());

	Ok(out)
}
