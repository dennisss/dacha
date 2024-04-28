// Utilities for dealing for sets of bits and bit stream I/O.

#[cfg(feature = "alloc")]
use alloc::string::String;
#[cfg(feature = "alloc")]
use alloc::vec::Vec;
use std::io::{Read, Write};
use std::string::ToString;

use crate::ceil_div;
use crate::errors::*;

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

/// Represents a variable length number of ordered bits
#[derive(PartialEq, Eq, Clone)]
pub struct BitVector {
    /// Number of bits stored in 'data'.
    len: usize,

    // TODO: std::mem::size_of::<Vec<u8>>() is 24, so let's inline any usage of up to 192 bits
    // which is good enough for most compression).
    /// Bits are stored from MSB to LSB in each individual byte.
    /// (in other words, bit index 0 is sotred in the MSB of data[0])
    data: Vec<u8>,
}

impl BitVector {
    /// Returns an empty vector.
    pub fn new() -> Self {
        BitVector {
            data: vec![],
            len: 0,
        }
    }

    pub fn clear(&mut self) {
        self.data.clear();
        self.len = 0;
    }

    pub fn set_all_zero(&mut self) {
        for v in &mut self.data {
            *v = 0;
        }
    }

    /// Appends a single bit to this vector.
    /// 'bit' must be 0 or 1
    pub fn push(&mut self, bit: u8) {
        assert!(bit <= 1);

        if self.len % 8 == 0 {
            self.data.push(0);
        }

        // NOTE: This assumes that all unused bits are 0.
        let last = self.data.last_mut().unwrap();
        *last |= bit << 7 - (self.len % 8);
        self.len += 1;
    }

    pub fn push_full_msb(&mut self, byte: u8) {
        assert!(self.len % 8 == 0);
        self.data.push(byte);
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
        let mut out = BitVector::new();
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
        let mut out = BitVector::new();
        for i in 0..width {
            out.push(((val >> (width - i - 1)) & 0b1) as u8);
        }

        assert_eq!(val >> width, 0);

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
        let mut data = Vec::from(data);
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

    pub fn from_raw_vec(data: Vec<u8>) -> Self {
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
    pub fn concat(&self, other: &BitVector) -> Self {
        let mut output = self.clone();
        for i in 0..other.len() {
            output.push(other.get(i).unwrap());
        }

        output
    }

    pub fn rotate_left(&self, n: usize) -> BitVector {
        let mut output = self.clone();
        for i in 0..self.len() {
            assert!(output.set(i, self.get((i + n) % self.len()).unwrap()));
        }

        output
    }

    pub fn xor(&self, other: &BitVector) -> BitVector {
        assert_eq!(self.len(), other.len());

        let mut output = self.clone();
        for i in 0..output.data.len() {
            output.data[i] ^= other.data[i];
        }

        output
    }

    pub fn split_at(&self, mid: usize) -> (BitVector, BitVector) {
        let mut left = BitVector::new();
        let mut right = BitVector::new();

        for i in 0..mid {
            left.push(self.get(i).unwrap());
        }

        for i in mid..self.len() {
            right.push(self.get(i).unwrap());
        }

        (left, right)
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
impl std::convert::AsRef<[u8]> for BitVector {
    fn as_ref(&self) -> &[u8] {
        &self.data
    }
}

impl std::fmt::Debug for BitVector {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut s = String::new();
        for i in 0..self.len() {
            s += &self.get(i).unwrap().to_string();
        }

        write!(f, "'{}'", s)
    }
}

/// Any string of '0' and '1' characters can be converted to a BitVector.
impl std::convert::TryFrom<&'_ str> for BitVector {
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
/// For reading many bytes, Read is also implemented, but it is invalid to use
/// Read until a multiple of 8 bits have been partially read.
pub struct BitReader<'a> {
    /// Base reader from which we will pull full bytes.
    reader: &'a mut dyn Read,

    /// Order in which to pull
    /// NOTE: This only effects reading partial partial bytes
    bit_order: BitOrder,
    /*
    We want to keep this byte aligned to
    */
    /// Bits read from the 'reader' which haven't yet been given to the user.
    ///
    /// NOTE: This may continue a non-multiple-of-8 bits if we called load()
    /// after into_unconsumed_bits() with a non-exact number of bytes being
    /// read.
    ///
    /// TODO:
    buffer: BitVector,

    /// Offset from 0-N bits within the buffer at which the next read will
    /// occur. Usually N will be 7 if no errors occur which would cause more
    /// than 8 bits to be buffered.
    offset: usize,

    /// How many bits were consumed (aka we can drop all bits before this point)
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

        self.buffer = bits;

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

        // TODO: Instead implement as a read from up to two bytes.
        let mut out = 0;
        for i in 0..n {
            if self.offset == self.buffer.len() {
                let mut buf = [0u8; 1];
                let nread = self.reader.read(&mut buf)?;
                // TODO: Annotate this if-statement with 'unlikely branch prediciton'
                if nread == 0 {
                    if i == 0 {
                        return Ok(None);
                    } else {
                        // Rollback and store all the bits we've read.
                        // TODO: In this case, reset the offset?

                        return Err(BitIoError::NotEnoughBits.into());
                    }
                }

                // TODO: Optimize this into full byte pushes.
                match self.bit_order {
                    BitOrder::LSBFirst => {
                        // Push bits into buffer from LSB to MSB
                        let mut b = buf[0];
                        for _ in 0..8 {
                            self.buffer.push(b & 0b01);
                            b = b >> 1;
                        }
                    }
                    BitOrder::MSBFirst => {
                        // TODO: WE should be able to simplify this to just pushing to the back of
                        // the BitVector's internal buffer?
                        let mut b = buf[0];
                        self.buffer.push_full_msb(b);
                        /*
                        for i in 0..8 {
                            self.buffer.push((b >> (7 - i)) & 0b1);
                        }
                        */
                    }
                }
            }

            out = out | ((self.buffer.get(self.offset).unwrap() as usize) << i);
            self.offset += 1;
        }

        Ok(Some(out))
    }

    pub fn read_bits_exact(&mut self, n: u8) -> Result<usize> {
        // TODO: This error should also be identified.
        self.read_bits(n)?
            .ok_or_else(|| BitIoError::NotEnoughBits.into())
    }

    // TODO: Integrate into read_bits so that this is faster
    pub fn read_bits_be(&mut self, n: u8) -> Result<usize> {
        let mut out = 0;
        for i in 0..n {
            let next_bit = self.read_bits_exact(1)?;
            out = (out << 1) | next_bit;
        }

        Ok(out)
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
        // NOTE: We assign based on distance to the end of the buffer as the start may
        // contain a partially read byte from a past call to into_unconsumed_bits().
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

impl Read for BitReader<'_> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        // TODO: A lot of this code assumes that we are storing bits MSB first.

        // NOTE: Because we always push full bytes into the end of the buffer, the end
        // of the buffer will always be aligned to an underlying byte offset.
        if (self.buffer.len() - self.offset) % 8 != 0 {
            println!("{} {}", self.buffer.len(), self.offset);
            return Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                "BitReader not aligned to a whole byte offset: regular reading not supported",
            ));
        }

        for i in 0..buf.len() {
            let res = match self.bit_order {
                BitOrder::LSBFirst => self.read_bits_exact(8),
                BitOrder::MSBFirst => self.read_bits_be(8),
            };

            let b = match res {
                Ok(v) => v as u8,
                Err(e) => {
                    if let Some(BitIoError::NotEnoughBits) = e.downcast_ref() {
                        break;
                    }

                    return Err(std::io::Error::new(
                        std::io::ErrorKind::Other,
                        "Unknown error",
                    ));
                }
            };

            buf[i] = b;
        }

        Ok(buf.len())
    }
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
pub struct BitWriter<'a> {
    order: BitOrder,
    writer: &'a mut dyn Write,
    bit_offset: u8,
    current_byte: u8,
}

impl<'a> BitWriter<'a> {
    pub fn new(writer: &'a mut dyn Write) -> Self {
        Self::new_with_order(writer, BitOrder::LSBFirst)
    }

    pub fn new_with_order(writer: &'a mut dyn Write, order: BitOrder) -> Self {
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
        for i in 0..self.bit_offset {
            out.push((v & 0b1) as u8);
            v = v >> 1;
        }

        out
    }
}

impl BitWrite for BitWriter<'_> {
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
            reader.read(&mut buf)?;

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
            reader.read(&mut buf)?;

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
            reader.read(&mut buf)?;

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
            reader.read(&mut buf)?;

            assert_eq!(&buf, &[0xCC]);
        }

        Ok(())
    }
}
