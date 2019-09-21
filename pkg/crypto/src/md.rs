

const CHUNK_SIZE: usize = 64;

const BITS_PER_BYTE: usize = 8;

const MESSAGE_LENGTH_BITS: usize = 64;

// type HashData<'a> = &'a mut [u32; 5];
pub type ChunkData = [u8; CHUNK_SIZE];
pub trait HashState = Clone;
pub trait CompressionFn<S: HashState> = Fn(&ChunkData, &mut S);

/// Helper for building hashes that use the Merkle–Damgård construction.
/// In particular, this requires a separate implementation of a compression
/// function implemented for a single block. Chunking the data will be handled
/// by this helper.
/// 
/// Block size: 512bit
/// 
/// Padding:
/// - Appends '1' bit to end of message
/// - Pads up to block_size - 64bits
/// - Appends message length in bits as 64bit integer (big endian)
#[derive(Clone)]
pub struct MerkleDamgard<S: HashState> {
	/// Current hash state produced by the last compression function or the IV.
	hash: S,
	/// Total number of *bytes* seen so far.
	length: usize,
	/// Unprocessed data in the last incomplete chunk. Will be processed once
	/// enough data is received to fill an entire chunk.  
	pending_chunk: ChunkData,
	/// Used for encoding the length.
	big_endian: bool
}

// TODO: Generalize the padding function as well.

// TODO: Support more than 2^64 bits by going wrapping adds and muls.

impl<S: HashState> MerkleDamgard<S> {
	pub fn new(iv: S, big_endian: bool) -> Self {
		Self { hash: iv, length: 0, pending_chunk: [0u8; CHUNK_SIZE],
			   big_endian }
	}

	pub fn update<F: CompressionFn<S>>(&mut self, mut data: &[u8], f: F) {
		// If the previous chunk isn't complete, try completing it.
		let rem = self.length % CHUNK_SIZE;
		if rem != 0 {
			let n = std::cmp::min(CHUNK_SIZE - rem, data.len());
			self.pending_chunk[rem..(rem + n)]
				.copy_from_slice(&data[0..n]);
			self.length += n;
			data = &data[n..];

			// Stop early if we did not fill the previous chunk.
			if self.length % CHUNK_SIZE != 0 {
				return;
			}

			f(&self.pending_chunk, &mut self.hash);
		}

		// Process all full chunks in the new data.
		for i in 0..(data.len() / CHUNK_SIZE) {
			f(array_ref![data, CHUNK_SIZE*i, CHUNK_SIZE],
			  &mut self.hash);
		}

		// Copy any remaining data into the pending chunk.
		let r = data.len() % CHUNK_SIZE;
		self.pending_chunk[0..r].copy_from_slice(
			&data[(data.len() - r)..]);

		// Update the length
		self.length += data.len();
	}

	// NOTE: Should use the same compression function as for updating.
	// The return value will be the internal state at the very end. Most likely
	// this will need to be post-processed for finalization.
	pub fn finish<F: CompressionFn<S>>(&self, f: F) -> S {
		// This will be the message length appended to the end of the message.
		let message_length = (BITS_PER_BYTE*self.length) as u64;

		// We need at least enough space to append the '1' bit and the 64bit 
		// message length. Then we will pad this up to the next chunk boundary.
		// NOTE: This is only valid as we are only operating on byte boundaries.
		let mut padded_len = self.length + (1 + 8);
		padded_len += common::block_size_remainder(
			MESSAGE_LENGTH_BITS as u64, padded_len as u64) as usize; 
		// Number of extra bytes that need to be added to the message to fit the 1 bit, message length and padding.
		let num_extra = padded_len - self.length;

		// Buffer allocated for the maximum number of extra bytes that we may need
		// we will only use num_extra at runtime.
		let mut buf = [0u8; CHUNK_SIZE + 9];

		// Append the '1' bit.
		buf[0] = 0x80;
		// Append the message length.
		*array_mut_ref![buf, num_extra - 8, 8] =
			if self.big_endian { message_length.to_be_bytes() }
			else { message_length.to_le_bytes() };

		let mut h = self.clone();
		h.update(&buf[0..num_extra], f);
		assert_eq!(h.length % CHUNK_SIZE, 0);

		h.hash
	}

}