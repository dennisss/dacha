

/*
	Compression strategies:
	- 
*/

use std::io::{Write};
use crate::bits::*;
use common::errors::*;
use byteorder::{LittleEndian, WriteBytesExt};
use std::collections::HashMap;
use std::collections::VecDeque;
use super::shared::*;
use crate::huffman::*;

/// Maximum size of the data contained in an uncompressed block.
const MAX_UNCOMPRESSED_BLOCK_SIZE: usize = u16::max_value() as usize;

// See https://github.com/madler/zlib/blob/master/deflate.c#L129 for all of zlib's parametes
const MAX_CHAIN_LENGTH: usize = 1024;

const MAX_MATCH_LENGTH: usize = 258;


// struct DeflateOptions {
// 	nice_match: u16,
// 	good_match: u16,
// 	lazy_match: u16

// }



// NOTE: Assumes that the header has already been written
fn write_uncompressed_block<W: BitWrite + Write>(data: &[u8], strm: &mut W)
	-> Result<()> {
	if data.len() > MAX_UNCOMPRESSED_BLOCK_SIZE {
		return Err("Data too long for uncompressed block".into());
	}

	let l = data.len() as u16;
	strm.write_u16::<LittleEndian>(l)?;
	strm.write_u16::<LittleEndian>(!l)?;
	strm.write_all(data)?;
	Ok(())
}

// Given a some data, can we tree to run length encode it

enum ReferenceEncoded {
	Literal(u8),
	LengthDistance(usize, usize)
}


pub struct CyclicBuffer {
	data: Vec<u8>,

	/// Absolute offset from before the first byte was ever inserted.
	/// This is essentially equivalent to the total number of bytes ever inserted
	/// during the lifetime of this buffer
	end_offset: usize
}

impl CyclicBuffer {
	pub fn new(size: usize) -> Self {
		assert!(size > 0);
		let mut data = vec![];
		data.resize(size, 0);
		CyclicBuffer { data, end_offset: 0 }
	}

	pub fn extend_from_slice(&mut self, mut data: &[u8]) {
		// Skip complete cycles of the buffer if the data is longer than the buffer.
		let nskip = (data.len() / self.data.len()) * self.data.len();
		self.end_offset += nskip;
		data = &data[nskip..];

		// NOTE: This will only ever have up to two iterations.
		while data.len() > 0 {
			let off = self.end_offset % self.data.len();
			let n = std::cmp::min(self.data.len() - off, data.len());
			(&mut self.data[off..(off + n)]).copy_from_slice(&data[0..n]);
			
			data = &data[n..];
			self.end_offset += n;
		}
	}

	/// The lowest absolute offset available in this 
	pub fn start_offset(&self) -> usize {
		if self.end_offset > self.data.len() {
			self.end_offset - self.data.len()
		} else {
			0
		}
	}

	pub fn end_offset(&self) -> usize {
		self.end_offset
	}

	pub fn slice_from(&self, start_off: usize) -> ConcatSlice {
		assert!(start_off >= self.start_offset()
				&& start_off < self.end_offset);
		
		let off = start_off % self.data.len();
		let mut n = self.end_offset - start_off;
		
		let rem = std::cmp::min(n, self.data.len() - off);
		let mut s = ConcatSlice::with(&self.data[off..(off+rem)]);
		n -= rem;

		if n > 0 {
			s = s.append(&self.data[0..n]);
		}

		s
	}
}

impl std::ops::Index<usize> for CyclicBuffer {
	type Output = u8;
	fn index(&self, idx: usize) -> &Self::Output {
		assert!(idx >= self.start_offset() &&
				idx < self.end_offset());

		let off = idx % self.data.len();
		&self.data[off]
	}
}


/// A slice like object consisting of multiple slices concatenated sequentially.
struct ConcatSlice<'a> {
	inner: Vec<&'a [u8]>
}

impl<'a> ConcatSlice<'a> {
	pub fn with(s: &'a [u8]) -> Self {
		ConcatSlice { inner: vec![s] }
	}

	pub fn append(mut self, s: &'a [u8]) -> Self {
		self.inner.push(s);
		self
	}

	pub fn len(&self) -> usize {
		self.inner.iter().map(|s| s.len()).sum()
	}
}

impl<'a> std::ops::Index<usize> for ConcatSlice<'a> {
	type Output = u8;
	fn index(&self, idx: usize) -> &Self::Output {
		let mut pos = 0;
		for s in self.inner.iter() {
			if idx - pos < s.len() {
				return &s[idx - pos];
			}

			pos += s.len();
		}

		panic!("Index out of range");
	}
}

struct AbsoluteReference {
	offset: usize,
	length: usize
}


type Trigram = [u8; 3];

/// A buffer of past uncompressed input which is 
pub struct MatchingWindow {
	buffer: CyclicBuffer,

	/// Map of three bytes in the back history to it's absolute position in the output buffer.
	/// 
	/// The linked list is maintained in order of descending 
	trigrams: HashMap<Trigram, VecDeque<usize>>
}

impl MatchingWindow {

	pub fn new() -> Self {
		MatchingWindow {
			buffer: CyclicBuffer::new(MAX_REFERENCE_DISTANCE),
			trigrams: HashMap::new()
		}
	}

	// TODO: keep track of the total number of trigrams in the window.
	// If this number gets too large, then perform a full sweep of the table to GC unused trigrams.

	/// NOTE: One should call this after the internal buffer has been updated. 
	/// NOTE: We assume that the given offset is larger than any previously inserted offset.
	fn insert_trigram(&mut self, gram: Trigram, offset: usize) {
		if let Some(list) = self.trigrams.get_mut(&gram) {
			// Enforce max chain length and discard offsets before the start of the current buffer.
			list.truncate(MAX_CHAIN_LENGTH);
			while let Some(last_offset) = list.back() {
				if *last_offset < self.buffer.start_offset() {
					list.pop_back();
				} else {
					break;
				}
			}

			// NOTE: No attempt is made to validate that this offset hasn't already been inserted.
			list.push_front(offset);

			if list.len() == 0 {
				self.trigrams.remove(&gram);
			}

		} else {
			let mut list = VecDeque::new();
			list.push_back(offset);
			self.trigrams.insert(gram, list);
		}
	}

	/// Given the next segment of uncompressed data, pushes it to the end of the window and in the process removing any data farther back the window size. 
	pub fn extend_from_slice(&mut self, data: &[u8]) {
		self.buffer.extend_from_slice(data);

		// Index of the first new trigram 
		let mut first = self.buffer.end_offset().checked_sub(data.len() + 2)
			.unwrap_or(0);
		if first < self.buffer.start_offset() {
			first = self.buffer.start_offset();
		}

		// Index of the last new trigram.
		let last = self.buffer.end_offset().checked_sub(2)
			.unwrap_or(0);

		for i in first..last {
			let gram = [self.buffer[i], self.buffer[i+1], self.buffer[i+2]];
			self.insert_trigram(gram, i);
		}
	}

	pub fn find_match(&self, data: &[u8]) -> Option<Reference> {
		if data.len() < 3 {
			return None;
		}

		let mut best_match: Option<AbsoluteReference> = None;

		let gram = [data[0], data[1], data[2]];
		let offsets = match self.trigrams.get(&gram) {
			Some(l) => l,
			None => { return None; }
		};

		for off in offsets {
			let s = self.buffer.slice_from(*off).append(data);

			// We trivially hae at least a match of 3 because we matched the trigram.
			let mut len = 3;
			for i in 3..s.len() {
				if i >= MAX_MATCH_LENGTH || i >= data.len() || s[i] != data[i] {
					len = i;
					break;
				}
			}

			if let Some(m) = &best_match {
				// NOTE: Even if they are equal, we prefer to use a later lower distance match of the same length.
				if m.length > len {
					continue;
				}
			}

			best_match = Some(AbsoluteReference { offset: *off, length: len });
		}

		// Converting from absolute offset to relative distance.
		best_match.map(|r| Reference {
			distance: self.buffer.end_offset() - r.offset,
			length: r.length
		})
	}
}

// TODO: Return an iterator.
fn reference_encode(window: &mut MatchingWindow,
					 data: &[u8]) -> Vec<ReferenceEncoded> {
	let mut out = vec![];

	let mut i = 0;
	while i < data.len() {
		let mut n = 1;
		if let Some(m) = window.find_match(&data[i..]) {
			n = m.length;
			out.push(ReferenceEncoded::LengthDistance(m.length, m.distance));
		} else {
			out.push(ReferenceEncoded::Literal(data[i]));
		}

		window.extend_from_slice(&data[i..(i+n)]);
		i += n;
	}

	assert_eq!(i, data.len());

	out
}

#[derive(Debug)]
enum CodeLengthAtom {
	Symbol(u8),
	ExtraBits(BitVector)
}

// TODO: If the lens list ends in 0's then we don't really need to encode it
/// Given the encoded code lengths for the dynamic length/literal and distance code trees, this will encode/compress them into the code length alphabet and write them to the output stream
fn append_dynamic_lens(lens: &[usize]) -> Result<Vec<CodeLengthAtom>> {

	let mut out = vec![];

	// We can only encode code lengths up to 15.
	for len in lens {
		if *len > MAX_LITLEN_CODE_LEN {
			return Err("Length is too long".into());
		}
	}

	let mut i = 0;
	while i < lens.len() {
		let v = lens[i];

		// Look for a sequence of zeros.
		if v == 0 {
			let mut j = i + 1;
			let j_max = std::cmp::min(lens.len(), i + 138);
			while j < lens.len() && lens[j] == 0 {
				j += 1;
			}

			let n = j - i;
			if n >= 11 {
				out.push(CodeLengthAtom::Symbol(18));
				out.push(CodeLengthAtom::ExtraBits(
					BitVector::from_usize(n - 11, 7)));
				
				i += n;
				continue;
			} else if n >= 3 {
				out.push(CodeLengthAtom::Symbol(17));
				out.push(CodeLengthAtom::ExtraBits(
					BitVector::from_usize(n - 3, 3)));
				
				i += n;
				continue;
			}
		}

		// Look for a sequence of repeated lengths
		if i > 0 && lens[i - 1] == v {
			let mut j = i + 1;
			let j_max = std::cmp::min(lens.len(), i + 6); // We can only encode up to 6 repetitions
			while j < j_max && lens[j] == v {
				j += 1;
			}

			let n = j - i;
			if n >= 3 {
				out.push(CodeLengthAtom::Symbol(16));
				out.push(CodeLengthAtom::ExtraBits(
					BitVector::from_usize(n - 3, 2)));
				
				i += n;
				continue;
			}
		}


		// Otherwise, just encode as a plain length
		out.push(CodeLengthAtom::Symbol(v as u8));
		i += 1;
	}

	Ok(out)
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn append_dynamic_lens_test() {
		let input = [0,0,0,0,3,4,5,5,5,5,5,5,12,5];
		let out = append_dynamic_lens(&input).unwrap();
		println!("{:?}", out);
	}

}


// TODO: Never flush when not on an even byte position.

/// Once the compressor has received this many bytes, it will begin generating a block.
const CHUNK_SIZE: usize = 8192;


/// All code length code lengths must fit in three bits (0-7).
const MAX_CODE_LEN_CODE_LEN: usize = 0b111;

pub fn write_deflate(data: &[u8], writer: &mut dyn Write) -> Result<()> {
	let mut strm = BitWriter::new(writer);
	let mut window = MatchingWindow::new();

	let mut i = 0;
	while i < data.len() {
		// Get a single chunk.
		let chunk = {
			let n = std::cmp::min(CHUNK_SIZE, data.len() - i);
			let c = &data[i..(i + n)];
			i += n;
			c
		};

		let is_final = i == data.len();

		strm.write_bits(if is_final { 1 } else { 0 }, 1)?;
		strm.write_bits(BTYPE_DYNAMIC_CODES as usize, 2)?;

		// Perform run length encoding.
		let codes = reference_encode(&mut window, data);

		// Convert to atoms
		let mut atoms = vec![];
		for c in codes.into_iter() {
			match c {
				ReferenceEncoded::Literal(v) => {
					append_lit(v, &mut atoms)?;
				},
				ReferenceEncoded::LengthDistance(len, dist) => {
					append_len(len, &mut atoms)?;
					append_distance(dist, &mut atoms)?;
				}
			}
		}

		append_end_of_block(&mut atoms);

		// Build huffman trees (will need to partially extract codes)
		let mut litlen_symbols = vec![];
		let mut dist_symbols = vec![];
		for a in atoms.iter() {
			match a {
				Atom::LitLenCode(c) => litlen_symbols.push(*c),
				Atom::DistCode(c) => dist_symbols.push(*c),
				Atom::ExtraBits(_) => {}
			};
		}

		let mut litlen_lens = dense_symbol_lengths(
			&HuffmanTree::build_length_limited_tree(
				&litlen_symbols, MAX_LITLEN_CODE_LEN)?
		);
		if litlen_lens.len() < 257 {
			litlen_lens.resize(257, 0);
		}

		let hlit = litlen_lens.len() - 257;
		strm.write_bits(hlit, 5)?;

		let mut dist_lens = dense_symbol_lengths(
			&HuffmanTree::build_length_limited_tree(
				&dist_symbols, MAX_LITLEN_CODE_LEN)?
		);
		if dist_lens.len() < 1 {
			dist_lens.resize(1, 0);
		}

		let hdist = dist_lens.len() - 1;
		strm.write_bits(hdist, 5)?;

		let mut code_lens = vec![];
		code_lens.extend_from_slice(&litlen_lens);
		code_lens.extend_from_slice(&dist_lens);

		let code_len_atoms = append_dynamic_lens(&code_lens)?;
		let code_len_symbols = code_len_atoms.iter().filter_map(|a| {
			match a {
				CodeLengthAtom::Symbol(u) => Some(*u as usize),
				_ => None
			}
		}).collect::<Vec<_>>();

		let sparse_code_len_code_lens = &HuffmanTree::build_length_limited_tree(&code_len_symbols, MAX_CODE_LEN_CODE_LEN)?;

		// Reorder the lengths and write to stream.
		{
			let mut ordering_inv = [0u8; CODE_LEN_ALPHA_SIZE];
			for (i, v) in CODE_LEN_CODE_LEN_ORDERING.iter().enumerate() {
				ordering_inv[*v as usize] = i as u8;
			}

			// TODO: There should be no need to do comparisons as we know the offsets
			let mut reordered = [0u8; CODE_LEN_ALPHA_SIZE];
			let mut reordered_len = 0;
			for v in sparse_code_len_code_lens.iter() {
				let i = ordering_inv[v.symbol] as usize;
				reordered[i] = v.length as u8;
				reordered_len = std::cmp::max(reordered_len, i + 1);
			}

			if reordered_len < 4 {
				reordered_len = 4;
			}

			let hclen = reordered_len - 4;
			strm.write_bits(hclen, 4)?;

			for len in &reordered[0..reordered_len] {
				strm.write_bits(*len as usize, 3)?;
			}
		}

		let code_len_code_lens = dense_symbol_lengths(
			sparse_code_len_code_lens
		);

		let code_len_encoder = HuffmanEncoder::from_canonical_lens(
			&code_len_code_lens)?;

		for atom in code_len_atoms.into_iter() {
			match atom {
				CodeLengthAtom::Symbol(s) => {
					code_len_encoder.write_symbol(s as usize, &mut strm);
				},
				CodeLengthAtom::ExtraBits(v) => {
					strm.write_bitvec(&v)?;
				}
			};
		}

		let litlen_encoder = HuffmanEncoder::from_canonical_lens(&litlen_lens)?;
		let dist_encoder = HuffmanEncoder::from_canonical_lens(&dist_lens)?;

		// Now write the actual data in this block
		for atom in atoms.into_iter() {
			match atom {
				Atom::LitLenCode(c) => litlen_encoder.write_symbol(c, &mut strm)?,
				Atom::DistCode(c) => dist_encoder.write_symbol(c, &mut strm)?,
				Atom::ExtraBits(v) => strm.write_bitvec(&v)?
			};
		}
		

		// Then we can build the code length huffman tree

		// TODO: If the histogram is sufficiently similar to the fixed tree one, then use the fixed tree

		// TODO: If the histogram and number of run-length encoded bytes is sufficiently small, use a heuristic to move to a no compression block instead
	}
	
	strm.finish()?;

	Ok(())
}

