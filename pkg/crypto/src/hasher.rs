#[cfg(feature = "alloc")]
use alloc::boxed::Box;
#[cfg(feature = "alloc")]
use alloc::vec::Vec;
#[cfg(feature = "std")]
use std::io::Read;

#[cfg(feature = "alloc")]
use common::factory::*;

pub trait StaticHasher {
    type DigestBuffer;
    type DigestPrimitive;
}

/// Encapsulates an algorithm for creating hashes (i.e. MD5, SHA1, CRC32, etc.).
/// TODO: Rename to Digest(er) to not conflict with the std::hash::Hasher
pub trait Hasher: Send {
    fn block_size(&self) -> usize;

    /// Should return the expected size of the output digest in bytes.
    fn output_size(&self) -> usize;

    /// Appends some data to the internal state of the hasher.
    fn update(&mut self, data: &[u8]);

    // TODO: Output into a provided buffer and

    /// Finalizes the hash and outputs the full hash of all data accumulated by
    /// calls to update(). The provided buffer must have at least output_size
    /// bytes.
    ///
    /// NOTE: If is valid to call update() after finish() is called (in which
    /// case all further calls to finish() will still be cumulative since the
    /// construction of this struct).
    #[cfg(feature = "alloc")]
    fn finish(&self) -> Vec<u8>;

    #[cfg(feature = "alloc")]
    fn finish_with(&mut self, data: &[u8]) -> Vec<u8> {
        self.update(data);
        self.finish()
    }

    /// Should create a cloned copy of this hasher such that the new and old
    /// hashers effectively have also data seen by update() applied already.
    #[cfg(feature = "alloc")]
    fn box_clone(&self) -> Box<dyn Hasher>;
}

#[cfg(feature = "alloc")]
pub type HasherFactory = Box<dyn Factory<dyn Hasher>>;

#[cfg(feature = "alloc")]
pub struct DefaultHasherFactory<T: Default + ?Sized> {
    t: std::marker::PhantomData<T>,
}

#[cfg(feature = "alloc")]
impl<T: Default + ?Sized> DefaultHasherFactory<T> {
    pub fn new() -> Self {
        Self {
            t: std::marker::PhantomData,
        }
    }
}

#[cfg(feature = "alloc")]
impl<T: Hasher + Default + Sync + ?Sized + 'static> Factory<dyn Hasher>
    for DefaultHasherFactory<T>
{
    fn create(&self) -> Box<dyn Hasher> {
        Box::new(T::default())
    }

    fn box_clone(&self) -> HasherFactory {
        Box::new(Self::new())
    }
}

#[cfg(feature = "alloc")]
pub trait GetHasherFactory {
    fn factory() -> HasherFactory;
}

#[cfg(feature = "alloc")]
impl<T: 'static + Default + Sync + Hasher> GetHasherFactory for T {
    fn factory() -> HasherFactory {
        Box::new(DefaultHasherFactory::<T>::new())
    }
}

/// Wrapper around a reader that calculates a checksum as you read.
#[cfg(feature = "std")]
pub struct HashReader<'a, H> {
    reader: &'a mut dyn Read,
    hasher: H,
}

#[cfg(feature = "std")]
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

#[cfg(feature = "std")]
impl<H: Hasher> Read for HashReader<'_, H> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let n = self.reader.read(buf)?;
        self.hasher.update(&buf[0..n]);
        Ok(n)
    }
}

/// Creates a hasher from an existing hasher by truncating the output to
/// a given length.
#[derive(Default, Clone)]
pub struct TruncatedHasher<H: Hasher, N: typenum::Unsigned + Send> {
    inner: H,
    _output_size: core::marker::PhantomData<N>,
}

impl<H: 'static + Hasher + Clone, N: 'static + typenum::Unsigned + Send + Clone> Hasher
    for TruncatedHasher<H, N>
{
    fn block_size(&self) -> usize {
        self.inner.block_size()
    }

    fn output_size(&self) -> usize {
        // TODO: Will be wrong if the inner hasher is smaller than given length.
        N::to_usize()
    }

    fn update(&mut self, data: &[u8]) {
        self.inner.update(data);
    }

    #[cfg(feature = "alloc")]
    fn finish(&self) -> Vec<u8> {
        let mut data = self.inner.finish();
        data.truncate(self.output_size());
        data
    }

    #[cfg(feature = "alloc")]
    fn box_clone(&self) -> Box<dyn Hasher> {
        Box::new(self.clone())
    }
}
