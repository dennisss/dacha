use std::collections::VecDeque;

use common::bytes::Bytes;

/// Rope-like chain of contiguous byte buffers.
///  
/// - Writers may push new data chunks to the end/tail of the queue or drop
///   unneeeded data from the start/head of the queue.
/// - The queue is defined as a view of absolute byte offsets [i, j) where i=0
///   is the first byte ever pushed into the buffer.
/// - Readers similarly read at absolute positions (and may fail if the absolute
///   position has been dropped from the queue already).
///
/// Compared to a coventional tree based rope, most operations are 'O(1)' as we
/// don't support random position reads or insertions..
pub struct BufferQueue {
    chunks: VecDeque<(usize, Bytes)>,

    first_chunk_index: usize,

    /// When chunks is not empty, this will equal to chunks[0].0
    first_byte_offset: usize,
}

/// Position in a BufferQueue. By default starts at the very beginning of the
/// buffer.
///
/// NOTE: It is undefined behavior to use a single cursor instance across
/// different BufferQueue instances.
#[derive(Default, Debug)]
pub struct BufferQueueCursor {
    /// While this could be derived from the list of chunks and the byte_offset,
    /// this is maintained as an optimization across reads.
    chunk_index: usize,

    byte_offset: usize,
}

impl BufferQueue {
    pub fn new() -> Self {
        Self {
            chunks: VecDeque::new(),
            first_chunk_index: 0,
            first_byte_offset: 0,
        }
    }

    pub fn start_byte_offset(&self) -> usize {
        self.first_byte_offset
    }

    //
    pub fn end_byte_offset(&self) -> usize {
        self.chunks
            .back()
            .map(|(i, b)| *i + b.len())
            .unwrap_or(self.first_byte_offset)
    }

    pub fn len(&self) -> usize {
        self.end_byte_offset() - self.start_byte_offset()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn push(&mut self, chunk: Bytes) {
        if chunk.is_empty() {
            return;
        }

        self.chunks.push_back((self.end_byte_offset(), chunk));
    }

    /// Removes chunks from the start/head of the buffer until length of the
    /// buffer is <= max_length.
    pub fn advance_until_under_limit(&mut self, max_length: usize) {
        while self.len() > max_length {
            let (_, buffer) = self.chunks.pop_front().unwrap();
            self.first_chunk_index += 1;
            self.first_byte_offset += buffer.len();
        }
    }

    pub fn advance(&mut self, cursor: &BufferQueueCursor) {
        while self.first_chunk_index < cursor.chunk_index {
            let (_, buffer) = self.chunks.pop_front().unwrap();
            self.first_chunk_index += 1;
            self.first_byte_offset += buffer.len();
        }
    }

    /// If this fails, then it means that advance_head() was called and removed
    /// data at the cursor position.
    pub fn read(&self, cursor: &mut BufferQueueCursor, mut buf: &mut [u8]) -> Result<usize, ()> {
        let mut nread = 0;

        if cursor.chunk_index < self.first_chunk_index {
            return Err(());
        }

        while !buf.is_empty() {
            let chunk_relative_index = cursor.chunk_index - self.first_chunk_index;
            if chunk_relative_index >= self.chunks.len() {
                break;
            }

            let (chunk_byte_offset, chunk_buffer) = &self.chunks[chunk_relative_index];

            let i = cursor.byte_offset - *chunk_byte_offset;
            let n = std::cmp::min(chunk_buffer.len() - i, buf.len());
            buf[0..n].copy_from_slice(&chunk_buffer[i..(i + n)]);

            buf = &mut buf[n..];

            if i + n == chunk_buffer.len() {
                cursor.chunk_index += 1;
            }
            cursor.byte_offset += n;
            nread += n;
        }

        Ok(nread)
    }
}
