use std::io::Read;

/// Encapsulates an algorithm for creating hashes (i.e. MD5, SHA1, CRC32, etc.).
pub trait Hasher {
	/// The type of the digest/cipher text for the finish hash.
	type Output;

	/// Appends some data to the internal state of the hasher.
	fn update(&mut self, data: &[u8]);
	
	/// Finalizes the hash and outputs the full hash of all data accumulated by calls to update().
	/// 
	/// NOTE: If is valid to call update() after finish() is called (in which case all further calls to finish() will still be cumulative since the construction of this struct).
	fn finish(&self) -> Self::Output;
}

/// Wrapper around a reader that calculates a checksum as you read.
pub struct HashReader<'a, H> {
	reader: &'a mut dyn Read,
	hasher: H
}

impl<T, H: Hasher<Output=T>> HashReader<'_, H> {
	pub fn new(reader: &mut dyn Read, hasher: H) -> HashReader<H> {
		HashReader { reader, hasher }
	}

	pub fn finish(&self) -> T {
		self.hasher.finish()
	}
}

impl<H: Hasher> Read for HashReader<'_, H> {
	fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
		let n = self.reader.read(buf)?;
		self.hasher.update(&buf[0..n]);
		Ok(n)
	}
}