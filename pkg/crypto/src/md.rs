use generic_array::{arr::AddLength, ArrayLength, GenericArray};
use std::ops::Add;
use typenum::Sum;
use typenum::U17;

const BITS_PER_BYTE: usize = 8;

pub trait HashState = Clone;
pub trait CompressionFn<S: HashState, N: ArrayLength<u8>> = Fn(&GenericArray<u8, N>, &mut S);

#[derive(Clone)]
pub struct LengthPadding {
    /// Whether or not to use big-endian when encoding the message length.
    pub big_endian: bool,
    /// Whether or not to use a u128 instead of a u64 when
    pub int128: bool,
}

pub trait ArrayLen = ArrayLength<u8> + Add<U17> + Clone;

/// Helper for building hashes that use the Merkle–Damgård construction.
/// In particular, this requires a separate implementation of a compression
/// function implemented for a single block. Chunking the data will be handled
/// by this helper.
///
/// Generic parameters:
/// - S: Compression function state type: arbitrary value that is passed to the
///   compression function with changes changes perspected
/// - ChunkSize: Size of each chunk/block that is processed. The compression
///   function will always be given chunks of this size.
///
/// Padding:
/// - Let N be the number of bits in the length marker (either 64 or 128)
/// - Appends '1' bit to end of message
/// - Pads up to block_size - N bits
/// - Appends message length in bits as an N-bit integer (big endian)
#[derive(Clone)]
pub struct MerkleDamgard<S: HashState, ChunkSize: ArrayLen> {
    /// Current hash state produced by the last compression function or the IV.
    hash: S,

    /// Total number of *bytes* seen so far.
    length: usize,

    /// Unprocessed data in the last incomplete chunk. Will be processed once
    /// enough data is received to fill an entire chunk.  
    pending_chunk: GenericArray<u8, ChunkSize>,

    length_padding: LengthPadding,
}

// TODO: Support more than 2^64 bits by going wrapping adds and muls.

impl<S: HashState, ChunkSize: ArrayLen> MerkleDamgard<S, ChunkSize> {
    pub fn new(iv: S, length_padding: LengthPadding) -> Self {
        // NOTE: Currently little endian int128 is not supported.
        if !length_padding.big_endian {
            assert!(!length_padding.int128);
        }

        Self {
            hash: iv,
            length: 0,
            pending_chunk: GenericArray::default(),
            length_padding,
        }
    }

    pub fn update<F: CompressionFn<S, ChunkSize>>(&mut self, mut data: &[u8], f: F) {
        // If the previous chunk isn't complete, try completing it.
        let rem = self.length % ChunkSize::to_usize();
        if rem != 0 {
            let n = std::cmp::min(ChunkSize::to_usize() - rem, data.len());
            self.pending_chunk[rem..(rem + n)].copy_from_slice(&data[0..n]);
            self.length += n;
            data = &data[n..];

            // Stop early if we did not fill the previous chunk.
            if self.length % ChunkSize::to_usize() != 0 {
                return;
            }

            f(&self.pending_chunk, &mut self.hash);
        }

        // Process all full chunks in the new data.
        for i in 0..(data.len() / ChunkSize::to_usize()) {
            let ii = i * ChunkSize::to_usize();
            let jj = ii + ChunkSize::to_usize();
            f((&data[ii..jj]).into(), &mut self.hash);
        }

        // Copy any remaining data into the pending chunk.
        let r = data.len() % ChunkSize::to_usize();
        self.pending_chunk[0..r].copy_from_slice(&data[(data.len() - r)..]);

        // Update the length
        self.length += data.len();
    }

    // NOTE: Should use the same compression function as for updating.
    // The return value will be the internal state at the very end. Most likely
    // this will need to be post-processed for finalization.
    pub fn finish<F: CompressionFn<S, ChunkSize>>(&self, f: F) -> S
    where
        Sum<ChunkSize, U17>: ArrayLength<u8>,
    {
        let message_length_bits = if self.length_padding.int128 { 128 } else { 64 };

        // This will be the message length appended to the end of the message.
        let message_length = (BITS_PER_BYTE * self.length) as u64;

        // We need at least enough space to append the '1' bit and the 64bit
        // message length. Then we will pad this up to the next chunk boundary.
        // NOTE: This is only valid as we are only operating on byte boundaries.
        let mut padded_len = self.length + common::ceil_div(1 + message_length_bits, 8);
        padded_len +=
            common::block_size_remainder(message_length_bits as u64, padded_len as u64) as usize;
        // Number of extra bytes that need to be added to the message to fit the 1 bit,
        // message length and padding.
        let num_extra = padded_len - self.length;

        // Buffer allocated for the maximum number of extra bytes that we may
        // need we will only use num_extra at runtime.
        // (this is calculated as chunk_size + 1byte + 128bit)
        // ^ this is excessive if we are only using a 64bit length, but its good enough.
        // TODO: Instead only allocat up to ChunkSize and update one chunk at
        // a time to simplify this.
        let mut buf = GenericArray::<u8, Sum<ChunkSize, U17>>::default();

        // Append the '1' bit.
        buf[0] = 0x80;
        // Append the message length.
        // NOTE: If using 128-bit integer, only 64-bits will be allowed padded
        // with zeros.
        *array_mut_ref![buf, num_extra - 8, 8] = if self.length_padding.big_endian {
            message_length.to_be_bytes()
        } else {
            message_length.to_le_bytes()
        };

        let mut h = self.clone();
        h.update(&buf[0..num_extra], f);
        assert_eq!(h.length % ChunkSize::to_usize(), 0);

        h.hash
    }
}
