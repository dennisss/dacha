


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
pub struct ConcatSlice<'a> {
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
