// Utilities for dealing for sets of bits and bit stream I/O.

#[cfg(feature = "alloc")]
use alloc::string::String;
#[cfg(feature = "alloc")]
use alloc::vec::Vec;
use std::io::{Read, Write};
use std::string::ToString;
use std::ops::{Index, IndexMut, Deref, DerefMut};

use crate::ceil_div;
use crate::errors::*;
use crate::list::List;

#[derive(Debug, Fail)]
pub enum BitIoError {
    /// Occurs when reading from a BitReader and the input stream runs out of
    /// bits before the read was complete.
    #[fail(display = "Not enough bits")]
    NotEnoughBits,
}

/// Sets a bit to either by 1 or 0 based on the given boolean.
/// TODO: Refactor so that 'val' is the last argument.
pub fn bitset(i: &mut u8, val: bool, bit: u8) {
    let mask = 1 << bit;
    *i = *i & !mask;
    if val {
        *i |= mask;
    }
}

/// Gets the value of a single bit in a byte (0 = false, 1 = true)
pub fn bitget(v: u8, bit: u8) -> bool {
    if v & (1 << bit) != 0 {
        true
    } else {
        false
    }
}

pub trait VecLike<T> {
    fn len(&self) -> usize;
    
    fn resize(&mut self, new_len: usize, value: T);

    fn last_mut(&mut self) -> Option<&mut T>;
}

impl<T: Clone> VecLike<T> for Vec<T> {
    fn len(&self) -> usize {
        Vec::<T>::len(self)
    }

    fn resize(&mut self, new_len: usize, value: T) {
        Vec::<T>::resize(self, new_len, value)
    }

    fn last_mut(&mut self) -> Option<&mut T> {
        self.as_mut_slice().last_mut()
    }
}

// pub trait BitVectorStorage = VecLike<u8> + List<u8> + Clone + Default + Index<usize, Output = u8> + IndexMut<usize> + for<'a> From<&'a [u8]> + AsRef<[u8]> + AsMut<[u8]>;

use crate::fixed::vec::FixedVec;

impl<T: Clone, const LEN: usize> VecLike<T> for crate::fixed::vec::FixedVec<T, LEN> {
    fn len(&self) -> usize {
        FixedVec::len(self)
    }

    fn resize(&mut self, new_len: usize, value: T) {
        FixedVec::resize(self, new_len, value)
    }

    fn last_mut(&mut self) -> Option<&mut T> {
        self.as_mut().last_mut()
    }
}


pub trait BitVectorStorage:
    Index<usize, Output = u8> + IndexMut<usize> +
    AsRef<[u8]> + AsMut<[u8]> +
    for<'a> From<&'a [u8]> +
    Clone + Default
{
    fn clear(&mut self);

    fn resize(&mut self, new_size: usize, value: u8);

    fn push(&mut self, index: usize, value: u8);

    fn len(&self) -> usize;
}

impl BitVectorStorage for Vec<u8> {
    fn clear(&mut self) {
        Vec::<u8>::clear(self)
    }

    fn resize(&mut self, new_size: usize, value: u8) {
        Vec::<u8>::resize(self, new_size, value)
    }

    #[inline(always)]
    fn push(&mut self, index: usize, value: u8) {
        Vec::<u8>::push(self, value)
    }

    fn len(&self) -> usize {
        Vec::<u8>::len(self)
    }
}




/// Represents a variable length number of ordered bits
#[derive(PartialEq, Eq, Clone)]
pub struct BitVector<T = Vec<u8>> {
    /// Number of bits stored in 'data'.
    len: usize,

    /// Bits are stored from MSB to LSB in each individual byte.
    /// (in other words, bit index 0 is stored in the MSB of data[0])
    ///
    /// TODO: std::mem::size_of::<Vec<u8>>() is 24, so let's inline any usage of up to 192 bits
    /// which is good enough for most compression).
    data: T,
}

impl<T: BitVectorStorage> BitVector<T> {
    /// Returns an empty vector.
    pub fn new() -> Self {
        Self {
            data: T::default(),
            len: 0,
        }
    }

    pub fn clear(&mut self) {
        self.data.clear();
        self.len = 0;
    }

    pub fn set_all_zero(&mut self) {
        for v in self.data.as_mut() {
            *v = 0;
        }
    }

    pub fn copy_from<Y: BitVectorStorage>(&mut self, other: &BitVector<Y>) {
        self.len = other.len();
        self.data.resize(other.data.len(), 0);
        self.data.as_mut().copy_from_slice(other.data.as_ref());
    }

    /// Appends a single bit to this vector.
    /// 'bit' must be 0 or 1
    pub fn push(&mut self, bit: u8) {
        assert!(bit <= 1);

        let idx = self.len / 8;
        if self.len % 8 == 0 {
            self.data.push(idx, 0);
        }

        // NOTE: This assumes that all unused bits are 0.
        self.data[idx] |= bit << 7 - (self.len % 8);
        self.len += 1;
    }

    pub fn push_full_msb(&mut self, byte: u8) {
        assert!(self.len % 8 == 0);
        self.data.push(self.len / 8, byte);
        self.len += 8;
    }

    /// Get the total number of bits stored in this vector.
    pub fn len(&self) -> usize {
        self.len
    }

    /// Get a single bit from the vector where the index is in the same order as
    /// the bit was push'ed.
    pub fn get(&self, i: usize) -> Option<u8> {
        if i >= self.len {
            return None;
        }

        Some((self.data[i / 8] >> (7 - (i % 8))) & 0b1)
    }

    pub fn set(&mut self, i: usize, value: u8) -> bool {
        if i >= self.len {
            return false;
        }

        bitset(&mut self.data[i / 8], value != 0, (7 - (i % 8)) as u8);

        true
    }

    pub fn get_byte(&self, bit_i: usize) -> Option<u8> {
        if bit_i + 8 > self.len {
            return None;
        }

        let byte_i = bit_i / 8;
        let rel_bit_i = bit_i % 8;

        let mut v = self.data[byte_i];
        if rel_bit_i != 0 {
            v <<= rel_bit_i;
            v |= self.data[byte_i + 1] >> rel_bit_i
        }

        Some(v)
    }

    /// Generates a bitvector from a number. The corresponding vector will start
    /// with the MSB of the number.
    ///
    /// TODO: Double check that all the usages of this are correct.
    ///
    /// MSB 0 0 0 0 0 0 0 0 LSB
    ///          [    <-   ]
    pub fn from_usize(val: usize, width: u8) -> Self {
        let mut out = Self::new();
        for i in 0..width {
            // NOTE: THis is not reversed!
            out.push(((val >> i) & 0b1) as u8);
        }

        // Assert 'val' has no more than width data in it.
        assert_eq!(val >> width, 0);

        out
    }

    /// MSB 0 0 0 0 0 0 0 0 LSB
    ///          [    ->   ]
    pub fn from_lower_msb(val: usize, width: u8) -> Self {
        let mut out = Self::new();
        for i in 0..width {
            out.push(((val >> (width - i - 1)) & 0b1) as u8);
        }

        assert_eq!(val >> width, 0);

        out
    }

    pub fn from_lower_msb_u16(val: u16, width: usize) -> Self {
        let mut out = Self::new();
        out.len = width;

        // NOTE: '0 << 16' will panic in debug mode.
        if width > 0 {
            let bytes = (val << (16 - width)).to_be_bytes();
            out.data.push(0, bytes[0]);
            out.data.push(1, bytes[1]);
        }

        out
    }

    pub fn to_lower_msb(&self) -> usize {
        let mut out = 0;

        for i in 0..self.len() {
            out = (out << 1) | (self.get(i).unwrap() as usize);
        }

        out
    }

    pub fn from_bytes(data: &[u8]) -> Self {
        Self::from(data, data.len() * 8)
    }

    pub fn from(data: &[u8], len: usize) -> Self {
        let mut data = T::from(data);
        data.resize(ceil_div(len, 8), 0);

        // Zero out any bits in the last byte that don't go up to 'len'
        let r = len % 8;
        if r != 0 {
            let i = data.len() - 1;
            let lastb = data[i];
            data[i] = (lastb >> (8 - r)) << (8 - r);
        }

        Self { data, len }
    }

    pub fn from_raw_vec(data: T) -> Self {
        let len = 8 * data.len();
        Self { data, len }
    }

    pub fn permute(&self, permutation: &[u8]) -> Self {
        let mut out = Self::new();
        for i in 0..permutation.len() {
            let j = permutation[i] as usize;
            out.push(self.get(j).unwrap());
        }
        out
    }

    /// Concatenates two bitvectors together.
    pub fn concat(&self, other: &Self) -> Self {
        let mut output = self.clone();
        for i in 0..other.len() {
            output.push(other.get(i).unwrap());
        }

        output
    }

    pub fn rotate_left(&self, n: usize) -> Self {
        let mut output = self.clone();
        for i in 0..self.len() {
            assert!(output.set(i, self.get((i + n) % self.len()).unwrap()));
        }

        output
    }

    pub fn xor(&self, other: &Self) -> Self {
        assert_eq!(self.len(), other.len());

        let mut output = self.clone();
        for i in 0..output.data.len() {
            output.data[i] ^= other.data[i];
        }

        output
    }

    pub fn split_at(&self, mid: usize) -> (Self, Self) {
        let mut left = Self::new();
        let mut right = Self::new();

        for i in 0..mid {
            left.push(self.get(i).unwrap());
        }

        for i in mid..self.len() {
            right.push(self.get(i).unwrap());
        }

        (left, right)
    }

    pub fn to_string(&self) -> String {
        let mut s = String::new();
        for i in 0..self.len() {
            s += &self.get(i).unwrap().to_string();
        }

        s
    }
}

#[derive(Clone, Copy)]
pub enum BitOrder {
    /// When reading, first take the highest (most significant) unread bit
    /// before proceeding to the next.
    MSBFirst,
    LSBFirst,
}

// TODO: THis will be wrong if we don't have a number of bits divisble by 8.
// ^ AKA: '1' should be encoded as '1' instead of as '0x80'
// NOTE: This should be guranteed to always minimally cover all bits up to the
// next complete octet.
impl<T: BitVectorStorage> std::convert::AsRef<[u8]> for BitVector<T> {
    fn as_ref(&self) -> &[u8] {
        self.data.as_ref()
    }
}

impl<T: BitVectorStorage> std::fmt::Debug for BitVector<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "'{}'", self.to_string())
    }
}

/// Any string of '0' and '1' characters can be converted to a BitVector.
impl<T: BitVectorStorage> std::convert::TryFrom<&'_ str> for BitVector<T> {
    type Error = Error;

    fn try_from(s: &str) -> Result<Self> {
        let mut out = BitVector::new();
        for c in s.chars() {
            if c == '0' {
                out.push(0);
            } else if c == '1' {
                out.push(1);
            } else {
                return Err(format_err!("Not 0|1: {}", c));
            }
        }

        Ok(out)
    }
}

/// Wrapper around a readable stream which allows for reading individual bits
/// from the stream at a time.
///
/// ALL READS ARE CACHED until the user calls consume() to allow discarding bits
/// that have been read.
///
/// For reading many bytes, read_bytes*() can be used, but it is invalid to use
/// these until a multiple of 8 bits have been partially read.
pub struct BitReader<'a> {
    /// Base reader from which we will pull full bytes.
    reader: &'a mut dyn Read,

    /// Order in which to pull
    /// NOTE: This only effects reading partial partial bytes
    bit_order: BitOrder,

    /// Bits read from the 'reader' which haven't yet been given to the
    /// user/consumed.
    ///
    /// NOTE: This is always contain a multiple of 8 bits since we read whole
    /// bytes at a time from the underlying reader.
    buffer: BitVector,

    /// Offset from 0-N bits within the buffer at which the next read will
    /// occur. Usually N will be 7 if no errors occur which would cause more
    /// than 8 bits to be buffered.
    offset: usize,

    /// How many bits were consumed (aka we can drop all bits before this point)
    consumed_offset: usize,
}

pub struct BitReaderRawState {
    buffer: BitVector,
    consumed_offset: usize,
}

// NOTE: THis reads from
impl<'a> BitReader<'a> {
    // TODO: Make MSBFirst the default as that is the most obvious.
    pub fn new(reader: &'a mut dyn Read) -> Self {
        Self::new_with_order(reader, BitOrder::LSBFirst)
    }

    pub fn new_with_order(reader: &'a mut dyn Read, bit_order: BitOrder) -> Self {
        Self {
            reader,
            offset: 0,
            buffer: BitVector::new(),
            consumed_offset: 0,
            bit_order,
        }
    }

    /// Pre-loads the reader with some set of bits which should be returned next
    /// on reads. Normally these will be bits retrieved from
    /// into_unconsumed_bits() from another BitReader instance.
    ///
    /// MUST be called immediately after new() and before any reads to this
    /// instance.
    pub fn load(&mut self, bits: BitVector) -> Result<()> {
        if self.offset != self.buffer.len() {
            return Err(err_msg("Already have pending bits loaded"));
        }

        self.buffer.clear();

        // Add zero padding so that when we later append 'bits', the end of the buffer
        // will be byte aligned.
        {
            let mut n = (bits.len() % 8);
            if n != 0 {
                n = 8 - n;
            }

            for i in 0..n {
                self.buffer.push(0);
            }
            self.offset = n;
            self.consumed_offset = n;
        }

        // Push the actual bits.
        for i in 0..bits.len() {
            self.buffer.push(bits.get(i).unwrap());
        }

        assert!(self.buffer.len() % 8 == 0);

        Ok(())
    }

    pub fn load_raw(&mut self, raw: BitReaderRawState) -> Result<()> {
        if self.offset != self.buffer.len() {
            return Err(err_msg("Already have pending bits loaded"));
        }

        self.buffer = raw.buffer;
        self.offset = raw.consumed_offset;
        self.consumed_offset = raw.consumed_offset;
        Ok(())
    }

    // TODO: Must support reading usize to read the lengths
    // TODO: This is heavily biased towards how zlib does it
    /// Reads a given number of bits from the stream and returns them as a byte.
    /// Up to 8 bits can be read.
    /// The final bit read will be in the most significant position of the
    /// return value.
    ///
    /// NOTE: Unless consume() is called, then this will accumulate bits
    /// indefinately
    ///
    /// NOTE: If an BitIoError::NotEnoughBits error occurs, then this operation
    /// is retryable if the reader later has all of the remaining bits.
    ///
    /// The return value will be None if and only if the first read bit is after
    /// the end of the file.
    pub fn read_bits(&mut self, n: u8) -> Result<Option<usize>> {
        // TODO: Can be implemented as a trivial read
        // But reading more than 8 bits can be tricky. Basially must loop
        // through bytes instead of through bits
        // if n < 8 - self.bit_offset {
        // 	let mask = (1 << n) - 1;

        // }

        match self.bit_order {
            BitOrder::MSBFirst => self.read_bits_msb(n),
            BitOrder::LSBFirst => self.read_bits_lsb(n),
        }
    }

    // TODO: Have a better way to keep this in sync with the other read_bits_xxx
    // function.
    fn read_bits_lsb(&mut self, n: u8) -> Result<Option<usize>> {
        // TODO: Instead implement as a read from up to two bytes.
        let mut out = 0;
        for i in 0..n {
            if self.offset == self.buffer.len() {
                let mut buf = [0u8; 1];
                let nread = self.reader.read(&mut buf)?;
                // This is unlikely since we normally operate with 100+ byte input buffers.
                if std::intrinsics::unlikely(nread == 0) {
                    if i == 0 {
                        return Ok(None);
                    } else {
                        // Rollback and store all the bits we've read.
                        // TODO: In this case, reset the offset?

                        return Err(BitIoError::NotEnoughBits.into());
                    }
                }

                // Push bits into buffer from LSB to MSB
                let mut b = buf[0];
                self.buffer.push_full_msb(b.reverse_bits());
            }

            // TODO: Ideally change this behavior so that it pushes to the MSB for MSBFirst
            // mode. Then we can get rid of the read_bits_be
            out = out | ((self.buffer.get(self.offset).unwrap() as usize) << i);
            self.offset += 1;
        }

        Ok(Some(out))
    }

    // TODO: Have a better way to keep this in sync with the other read_bits_xxx
    // function.
    fn read_bits_msb(&mut self, n: u8) -> Result<Option<usize>> {
        // TODO: Instead implement as a read from up to two bytes.

        while self.offset + (n as usize) > self.buffer.len() {
            let mut buf = [0u8; 1];
            let nread = self.reader.read(&mut buf)?;
            // This is unlikely since we normally operate with 100+ byte input buffers.
            if std::intrinsics::unlikely(nread == 0) {
                if self.offset == self.buffer.len() {
                    // There are zero more
                    return Ok(None);
                } else {
                    return Err(BitIoError::NotEnoughBits.into());
                }
            }

            let mut b = buf[0];
            self.buffer.push_full_msb(b);
        }

        /*
        Get the current byte and the next byte and shift/mask the appropriate amont.

        */

        let mut out = 0;
        for i in 0..n {
            out = (out << 1) | (self.buffer.get(self.offset).unwrap() as usize);
            self.offset += 1;
        }

        Ok(Some(out))
    }

    pub fn read_bits_exact(&mut self, n: u8) -> Result<usize> {
        // TODO: This error should also be identified.
        self.read_bits(n)?
            .ok_or_else(|| BitIoError::NotEnoughBits.into())
    }

    pub fn consume(&mut self) {
        // TODO: If we have very few bits, then it would also make sense to shift over
        // the buffer if we consume a round number of bits.
        if self.offset == self.buffer.len() {
            self.buffer.clear();
            self.offset = 0;
        }

        self.consumed_offset = self.offset;
    }

    /// Moves the cursor of the stream to the next full byte
    pub fn align_to_byte(&mut self) {
        self.offset += (self.buffer.len() - self.offset) % 8;
    }

    /// Outputs all remaining unread bits in the last read bytes.
    pub fn into_unconsumed_bits(self) -> BitVector {
        let mut buf = BitVector::new();
        for i in self.consumed_offset..self.buffer.len() {
            buf.push(self.buffer.get(i).unwrap());
        }

        buf
    }

    pub fn into_unconsumed_raw(self) -> BitReaderRawState {
        BitReaderRawState {
            buffer: self.buffer,
            consumed_offset: self.consumed_offset,
        }
    }

    /// Reads some number of complete bytes from the
    pub fn read_bytes(&mut self, buf: &mut [u8]) -> Result<usize> {
        // TODO: A lot of this code assumes that we are storing bits MSB first.

        // NOTE: Because we always push full bytes into the end of the buffer, the end
        // of the buffer will always be aligned to an underlying byte offset.
        if (self.buffer.len() - self.offset) % 8 != 0 {
            println!("{} {}", self.buffer.len(), self.offset);
            return Err(err_msg(
                "BitReader not aligned to a whole byte offset: regular reading not supported",
            ));
        }

        let mut num_read = 0;

        for i in 0..buf.len() {
            let res = self.read_bits_exact(8);

            let b = match res {
                Ok(v) => v as u8,
                Err(e) => {
                    if let Some(BitIoError::NotEnoughBits) = e.downcast_ref() {
                        break;
                    }

                    return Err(e);
                }
            };

            buf[i] = b;
            num_read += 1;
        }

        Ok(num_read)
    }

    pub fn read_bytes_exact(&mut self, buf: &mut [u8]) -> Result<()> {
        // TODO: Must loop if we got zero?
        let n = self.read_bytes(buf)?;
        if n != buf.len() {
            return Err(BitIoError::NotEnoughBits.into());
        }

        Ok(())
    }

    /// Faster version of read_bytes() which immediately consumes any read bytes
    /// without buffering them.
    pub fn read_bytes_and_consume(&mut self, buf: &mut [u8]) -> Result<usize> {
        // NOTE: Because we always push full bytes into the end of the buffer, the end
        // of the buffer will always be aligned to an underlying byte offset.
        if (self.buffer.len() - self.offset) % 8 != 0 {
            println!("{} {}", self.buffer.len(), self.offset);
            return Err(err_msg(
                "BitReader not aligned to a whole byte offset: regular reading not supported",
            ));
        }

        let mut num_read = 0;

        while num_read < buf.len() && self.offset < self.buffer.len() {
            let b = self.read_bits_exact(8)? as u8;

            buf[num_read] = b;
            num_read += 1;
        }

        self.consume();

        if num_read < buf.len() {
            num_read += self.reader.read(&mut buf[num_read..])?;
        }

        Ok(num_read)
    }
}

pub struct FastBitReader<'a> {
    reader: &'a mut dyn Read,
    buffer: u8,
    /// Number of bits remaining in the buffer
    buffer_left: u8,
}

impl<'a> FastBitReader<'a> {
    pub fn read_bit(&mut self) -> Result<u8> {
        if self.buffer_left == 0 {
            let mut buf = [0u8; 1];
            if self.reader.read(&mut buf)? != 1 {
                return Err(BitIoError::NotEnoughBits.into());
            }
            self.buffer_left = 8;
        }

        let (next, overflowed) = self.buffer.overflowing_shl(1);
        self.buffer = next;
        self.buffer_left -= 1;
        Ok(if overflowed { 1 } else { 0 })
    }
}

/*
impl<T> BitReader<'_, std::io::Cursor<T>> {
    ///
    ///
    /// Returns:
    /// (# of full bytes read,
    ///  # of bits read in the next byte after those)
    fn position(&self) -> (usize, usize) {
        let mut nbytes = self.reader.position();
        if self.bit_offset > 0 {
            nbytes -= 1;
        }

        (nbytes, self.bit_offset)
    }
}
*/

// TODO: Executally merge into 'write_bitvec'
pub trait BitWriteExt<T: BitVectorStorage>: BitWrite {
    fn write_bitvec_generic(&mut self, val: &BitVector<T>) -> Result<()> {
        for i in 0..val.len() {
            self.write_bits(val.get(i).unwrap() as usize, 1)?;
        }

        Ok(())
    }
}

impl<W: Write, T: BitVectorStorage> BitWriteExt<T> for BitWriter<'_, W> {

}

pub trait BitWrite {
    /// Writes the lowest 'len' bits of 'val' to this stream.
    fn write_bits(&mut self, val: usize, len: u8) -> Result<()>;

    fn write_bitvec(&mut self, val: &BitVector) -> Result<()> {
        for i in 0..val.len() {
            self.write_bits(val.get(i).unwrap() as usize, 1)?;
        }

        Ok(())
    }

    /// Immediately finish writing any partial bytes to the underlying stream.
    ///
    /// NOTE: This should always be called after using this stream to guarantee
    /// that everything has been written.
    fn finish(&mut self) -> Result<()>;
}

// TODO: This should also support different LSB or MSB styles.
pub struct BitWriter<'a, W: ?Sized = dyn Write> {
    order: BitOrder,
    writer: &'a mut W,
    bit_offset: u8,
    current_byte: u8,
}

impl<'a, W: ?Sized + Write> BitWriter<'a, W> {
    pub fn new(writer: &'a mut W) -> Self {
        Self::new_with_order(writer, BitOrder::LSBFirst)
    }

    pub fn new_with_order(writer: &'a mut W, order: BitOrder) -> Self {
        Self {
            order,
            writer,
            bit_offset: match order {
                BitOrder::LSBFirst => 0,
                BitOrder::MSBFirst => 7,
            },
            current_byte: 0,
        }
    }

    /// Obtains a bitvector that represents all pending bits inside of the
    /// writer. Calling write_bitvec() later on an empty BitWrite will
    /// return the BitWrite to the same state.
    pub fn into_bits(self) -> BitVector {
        let mut out = BitVector::new();
        let mut v = self.current_byte;

        match self.order {
            BitOrder::LSBFirst => {
                for i in 0..self.bit_offset {
                    out.push((v & 0b1) as u8);
                    v = v >> 1;
                }
            }
            BitOrder::MSBFirst => {
                for i in 0..(7 - self.bit_offset) {
                    out.push((v >> (7 - i)) & 1);
                }
            }
        }

        out
    }
}

impl<W: Write> BitWrite for BitWriter<'_, W> {
    fn write_bits(&mut self, mut val: usize, len: u8) -> Result<()> {
        match self.order {
            BitOrder::LSBFirst => {
                for i in 0..len {
                    self.current_byte |= ((val & 0b1) << self.bit_offset) as u8;
                    self.bit_offset += 1;
                    val = val >> 1;

                    if self.bit_offset == 8 {
                        self.finish()?;
                    }
                }
            }
            BitOrder::MSBFirst => {
                for i in 0..len {
                    // Next MSB in current_byte gets next MSB in input.
                    self.current_byte |= (((val >> (len - i - 1)) & 0b1) << self.bit_offset) as u8;

                    if self.bit_offset == 0 {
                        self.finish()?;
                    } else {
                        self.bit_offset -= 1;
                    }
                }
            }
        }

        // Ensure that 'val' doesn't contain more the 'len' bits
        // assert_eq!(val, 0);

        Ok(())
    }

    fn finish(&mut self) -> Result<()> {
        match self.order {
            BitOrder::LSBFirst => {
                if self.bit_offset > 0 {
                    let buf = [self.current_byte];
                    self.writer.write_all(&buf)?;
                    self.bit_offset = 0;
                    self.current_byte = 0;
                }
            }
            BitOrder::MSBFirst => {
                if self.bit_offset < 7 {
                    let buf = [self.current_byte];
                    self.writer.write_all(&buf)?;
                    self.bit_offset = 7;
                    self.current_byte = 0;
                }
            }
        }

        Ok(())
    }
}

/// This is an optimized version of BitWriter that only supports
/// supporting bits in MSB first order.
///
/// Internally it buffers up to 64bits at a time and individual write operations
/// MUST NOT exceed 56 bits in size without there being a risk of overflowing. 
pub struct BitWriter64<W> {
    writer: W,
    buffer: u64,
    bit_offset: usize,
}

impl<W: FnMut(&[u8])> BitWriter64<W> {
    pub fn new(writer: W) -> Self {
        Self {
            writer,
            buffer: 0,
            bit_offset: 0,
        }
    }

    pub fn write_bitvec_generic<T: BitVectorStorage>(&mut self, vec: &BitVector<T>) {

        let final_offset = self.bit_offset + vec.len();
        for byte in vec.as_ref() {
            self.buffer |= (*byte as u64) << (56 - self.bit_offset);
            self.bit_offset += 8;
        }
        self.bit_offset = final_offset;

        while self.bit_offset > 8 {
            let byte = (self.buffer >> 56) as u8;
            (self.writer)(&[byte]);

            self.buffer <<= 8;
            self.bit_offset -= 8;
        }
    }

    /// Optimized version of write_bitvec_generic for writing up to 16 bits at a time.
    ///
    /// This assumes that you will never only ever call this function and not write_bitvec_generic.
    pub fn write_bitvec_max16<T: BitVectorStorage>(&mut self, vec: &BitVector<T>) {
        let slice = vec.as_ref();
        
        let val = if slice.len() >= 2 {
            ((slice[0] as u64) << 8) | (slice[1] as u64)
        } else if slice.len() == 1 {
            (slice[0] as u64) << 8
        } else {
            return;
        };

        // NOTE: This subtraction will overflow if we have less than 16 bits of space left in
        // our buffer.
        self.buffer |= val << (48 - self.bit_offset);
        self.bit_offset += vec.len();

        let num_bytes = self.bit_offset / 8;
        if num_bytes >= 6 {
            let out = self.buffer.to_be_bytes();
            (self.writer)(&out[..num_bytes]);

            self.buffer <<= num_bytes * 8;
            self.bit_offset %= 8;
        }
    }

    pub fn finish(&mut self) {
        while self.bit_offset > 8 {
            let byte = (self.buffer >> 56) as u8;
            (self.writer)(&[byte]);
            self.buffer <<= 8;
            self.bit_offset -= 8;
        }

        if self.bit_offset != 0 {
            let byte = (self.buffer >> 56) as u8;
            (self.writer)(&[byte]);
            self.buffer <<= 8;
            self.bit_offset = 0;
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;

    #[test]
    fn bitvector_works() {
        let mut v = BitVector::new();
        let vals = vec![0, 1, 1, 0, 1, 0, 1, 1, 1, 0, 0];
        for i in 0..vals.len() {
            v.push(vals[i]);
            assert_eq!(v.len(), i + 1);
            for j in 0..(i + 1) {
                assert_eq!(v.get(j), Some(vals[j]));
            }
        }

        assert_eq!(&format!("{:?}", v), "'01101011100'");
    }

    #[test]
    fn bitwriter_test() {
        let mut data = Vec::new();
        let mut strm = BitWriter::new(&mut data);
        strm.write_bits(0b1, 1).unwrap();
        strm.write_bits(0b01, 2).unwrap();
        strm.finish().unwrap();

        assert_eq!(data[0], 0b011);
    }

    #[test]
    fn bitvector_set() {
        let mut v = BitVector::from(&[0, 0], 13);
        assert_eq!(v.len(), 13);
        assert_eq!(v.as_ref(), &[0, 0]);

        v.set(0, 1);
        assert_eq!(v.as_ref(), &[0x80, 0]);

        v.set(2, 1);
        assert_eq!(v.as_ref(), &[0xA0, 0]);

        v.set(11, 1);
        assert_eq!(v.as_ref(), &[0xA0, 0x10]);

        v.set(2, 0);
        assert_eq!(v.as_ref(), &[0x80, 0x10]);
    }

    #[test]
    fn bitvector_rotate_left() {
        let v = BitVector::from(&[0b11010000], 4);
        assert_eq!(v.rotate_left(1).as_ref(), &[0b10110000]);
        assert_eq!(v.rotate_left(3).as_ref(), &[0b11100000]);
    }

    #[test]
    fn bitvector_concat() {
        let a = BitVector::from(&[0xde, 0xff], 10);
        let b = BitVector::from(&[0b01011000], 6);
        assert_eq!(a.concat(&b).as_ref(), &[0xde, 0b11010110]);
    }

    #[test]
    fn bitreader_align_to_byte() -> Result<()> {
        // No-op at beginning
        {
            let data = &[0xAA, 0xBB];
            let mut cursor = Cursor::new(data);

            let mut reader = BitReader::new(&mut cursor);
            reader.align_to_byte();

            let mut buf = [0u8];
            assert_eq!(reader.read_bytes(&mut buf)?, 1);

            assert_eq!(&buf, &[0xAA]);
        }

        // After reading some bits
        {
            let data = &[0xAA, 0xBB];
            let mut cursor = Cursor::new(data);

            let mut reader = BitReader::new(&mut cursor);

            reader.read_bits_exact(2)?;
            reader.align_to_byte();

            let mut buf = [0u8];
            assert_eq!(reader.read_bytes(&mut buf)?, 1);

            assert_eq!(&buf, &[0xBB]);
        }

        // After loading a part of one byte.
        {
            let data = &[0xAA, 0xBB];
            let mut cursor = Cursor::new(data);

            let mut reader = BitReader::new(&mut cursor);
            reader.load(BitVector::from_usize(0, 2))?;

            reader.align_to_byte();

            let mut buf = [0u8];
            assert_eq!(reader.read_bytes(&mut buf)?, 1);

            assert_eq!(&buf, &[0xAA]);
        }

        // After loading 10 bits
        {
            let data = &[0xAA, 0xBB];
            let mut cursor = Cursor::new(data);

            let mut reader = BitReader::new_with_order(&mut cursor, BitOrder::MSBFirst);
            reader.load(BitVector::from_usize(0xCC, 10))?;

            reader.align_to_byte();

            let mut buf = [0u8];
            assert_eq!(reader.read_bytes(&mut buf)?, 1);

            assert_eq!(&buf, &[0xCC]);
        }

        // Consume some bytes from both the buffer and internal reader.
        {
            let data = &[0xAA, 0xBB];
            let mut cursor = Cursor::new(data);

            let mut reader = BitReader::new_with_order(&mut cursor, BitOrder::MSBFirst);
            reader.load(BitVector::from_usize(0xCC, 10))?;

            reader.align_to_byte();

            let mut buf = [0u8; 4];
            assert_eq!(reader.read_bytes(&mut buf)?, 3);

            assert_eq!(&buf, &[0xCC, 0xAA, 0xBB, 00]);
        }

        // Same thing as the previous case, but use read_bytes_and_consume
        {
            let data = &[0xAA, 0xBB];
            let mut cursor = Cursor::new(data);

            let mut reader = BitReader::new_with_order(&mut cursor, BitOrder::MSBFirst);
            reader.load(BitVector::from_usize(0xCC, 10))?;

            reader.align_to_byte();

            let mut buf = [0u8; 4];
            assert_eq!(reader.read_bytes_and_consume(&mut buf)?, 3);

            assert_eq!(&buf, &[0xCC, 0xAA, 0xBB, 00]);
        }

        Ok(())
    }
}
