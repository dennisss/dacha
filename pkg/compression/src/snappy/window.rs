use crypto::checksum::crc::crc32c_raw_oneshot;
use crypto::hasher::Hasher;

use crate::deflate::cyclic_buffer::WindowBuffer;
use crate::deflate::matching_window::{AbsoluteReference, RelativeReference};

const BLOCK_SIZE: usize = 1 << 16;

#[derive(Defaultable)]
pub struct MatchingWindowSnappyOptions {
    #[default(1 << 16)]
    pub table_size: usize,

    #[default(64)]
    pub max_match_length: usize,
}

/// Matching window for finding back references in a data buffer which is built
/// similarly to the original Snappy / Gipfeli implementation.
///
/// Uses a static size hash table:
/// - Key is stored implicitly as the vector index.
///   - Computed using a 'CRC32' of the next 4 bytes in the input buffer.
/// - Value is a u16 offset representing the position of the bytes associated
///   with the key.
///   - Computed by taking the lower 16 bits of the absolute offset to the
///     bytes.
///   - We assume that this offset corresponds to the last possible offset in
///     the 2^16 bytes before the current cursor position.
///
/// The hash table itself doesn't store any information about whether or not a
/// key/value pair is present/deleted. Instead, at lookup time, the user needs
/// to check if the bytes at the stored offset match the query.
///
/// A minimum of 4 bytes are allowed to be matched.
pub struct MatchingWindowSnappy<B: WindowBuffer> {
    options: MatchingWindowSnappyOptions,
    buffer: B,
    table: Vec<u16>,
}

impl<B: WindowBuffer> MatchingWindowSnappy<B> {
    pub fn new(buffer: B, options: MatchingWindowSnappyOptions) -> Self {
        let table = vec![0; options.table_size];
        Self {
            options,
            buffer,
            table,
        }
    }

    /// Gets the hash table bucket index corresponding to the first 4 bytes in
    /// the data.
    fn bucket_index(&self, data: &[u8; 4]) -> usize {
        let i = crc32c_raw_oneshot(u32::from_ne_bytes(*data));

        // TODO: Change to '& 0xFFFF' to optimize for a 64K entry hash table.
        (i as usize) % self.table.len()
    }

    // TODO: Ideally do this at the same time as finding matches.
    pub fn advance(&mut self, data: &[u8]) {
        self.buffer.extend_from_slice(data);

        let mut first = self
            .buffer
            .end_offset()
            .checked_sub(data.len() + 3)
            .unwrap_or(0);

        if first < self.buffer.start_offset() {
            first = self.buffer.start_offset();
        }

        let last = self.buffer.end_offset().checked_sub(3).unwrap_or(0);

        for i in first..last {
            let buf = [
                self.buffer[i],
                self.buffer[i + 1],
                self.buffer[i + 2],
                self.buffer[i + 3],
            ];

            let idx = self.bucket_index(&buf);

            self.table[idx] = (i % BLOCK_SIZE) as u16;
        }
    }

    pub fn find_match(&self, data: &[u8]) -> Option<RelativeReference> {
        if data.len() < 4 {
            return None;
        }

        let relative_offset = self.table[self.bucket_index(array_ref![data, 0, 4])];

        let block_start = (self.buffer.end_offset() / BLOCK_SIZE) * BLOCK_SIZE;

        let mut offset = block_start + (relative_offset as usize);
        if offset >= self.buffer.end_offset() {
            if offset < BLOCK_SIZE {
                return None;
            }

            offset -= BLOCK_SIZE;
        }

        // TODO: Deduplicate the below code with the other window implementation.

        // NOTE: Should always be non-empty because we would return None above if it
        // was.
        let s = self.buffer.slice_from(offset).append(data);

        let mut len = 0;
        for i in 0..s.len() {
            if i >= self.options.max_match_length || i >= data.len() || s[i] != data[i] {
                len = i;
                break;
            }
        }

        if len >= 4 {
            Some(RelativeReference {
                distance: self.buffer.end_offset() - offset,
                length: len,
            })
        } else {
            None
        }
    }
}
