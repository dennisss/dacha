use std::io::Read;
use common::factory::*;

/// Encapsulates an algorithm for creating hashes (i.e. MD5, SHA1, CRC32, etc.).
/// TODO: Rename to Digest(er) to not conflict with the std::hash::Hasher
pub trait Hasher: Send {

	// fn block_size() -> usize;

	/// Should return the expected size of the output digest in bytes.
	fn output_size(&self) -> usize;

	/// Appends some data to the internal state of the hasher.
	fn update(&mut self, data: &[u8]);
	
	// TODO: Output into a provided buffer and 

	/// Finalizes the hash and outputs the full hash of all data accumulated by calls to update(). The provided buffer must have at least output_size bytes. 
	/// 
	/// NOTE: If is valid to call update() after finish() is called (in which case all further calls to finish() will still be cumulative since the construction of this struct).
	fn finish(&self) -> Vec<u8>;
}

pub type HasherFactory = Box<dyn Factory<dyn Hasher>>;

pub struct DefaultHasherFactory<T: Default + ?Sized> {
	t: std::marker::PhantomData<T>
}

impl<T: Default + ?Sized> DefaultHasherFactory<T> {
	pub fn new() -> Self {
		Self { t: std::marker::PhantomData }
	}
}

impl<T: Hasher + Default + Sync + ?Sized + 'static> Factory<dyn Hasher> for DefaultHasherFactory<T> {
	fn create(&self) -> Box<dyn Hasher> {
		Box::new(T::default())
	}

	fn box_clone(&self) -> HasherFactory {
		Box::new(Self::new())
	}
}

pub trait GetHasherFactory {
	fn factory() -> HasherFactory;
}

impl<T: 'static + Default + Sync + Hasher> GetHasherFactory for T {
	fn factory() -> HasherFactory {
		Box::new(DefaultHasherFactory::<T>::new())
	}
}


/// Wrapper around a reader that calculates a checksum as you read.
pub struct HashReader<'a, H> {
	reader: &'a mut dyn Read,
	hasher: H
}

impl<H: Hasher> HashReader<'_, H> {
	pub fn new(reader: &mut dyn Read, hasher: H) -> HashReader<H> {
		HashReader { reader, hasher }
	}

	pub fn finish(&self) -> Vec<u8> {
		self.hasher.finish()
	}

	pub fn into_hasher(self) -> H {
		self.hasher
	}
}

impl<H: Hasher> Read for HashReader<'_, H> {
	fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
		let n = self.reader.read(buf)?;
		self.hasher.update(&buf[0..n]);
		Ok(n)
	}
}