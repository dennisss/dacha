

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


/// Maximum size of the data contained in an uncompressed block.
const MAX_UNCOMPRESSED_BLOCK_SIZE: usize = u16::max_value() as usize;

// See https://github.com/madler/zlib/blob/master/deflate.c#L129 for all of zlib's parametes
const MAX_CHAIN_LENGTH: usize = 1024;

const MAX_MATCH_LENGTH: usize = 258;

const MAX_DISTANCE: usize = 32768;

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

enum RunLengthEncoded {
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



#[derive(Debug)]
pub struct Run {
	pub distance: usize,
	pub length: usize
}

struct AbsoluteRun {
	offset: usize,
	length: usize
}



type Trigram = [u8; 3];

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
			buffer: CyclicBuffer::new(MAX_DISTANCE),
			trigrams: HashMap::new()
		}
	}

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

	pub fn find_match(&self, data: &[u8]) -> Option<Run> {
		if data.len() < 3 {
			return None;
		}

		let mut best_match: Option<AbsoluteRun> = None;

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

			best_match = Some(AbsoluteRun { offset: *off, length: len });
		}

		// Converting from absolute offset to relative distance.
		best_match.map(|r| Run {
			distance: self.buffer.end_offset() - r.offset,
			length: r.length
		})
	}
}


// Picking off deleted



// TODO: Can we reference things in a previous block?
// fn run_length_encode(data: &[u8]) -> Vec<>