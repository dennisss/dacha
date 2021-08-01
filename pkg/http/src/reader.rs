use common::bytes::Bytes;
use common::errors::*;
use common::io::*;

/// An object that matches some pattern in a stream of bytes.
pub trait Matcher {
    /// Considering all previous data passed to this function, try to find a
    /// match given a new chunk of data returning the index immediately after
    /// the first match.
    fn process(&mut self, data: &[u8]) -> Option<usize>;
}

// TODO: Move this somewhere else as this is very specific to http.
/// Matches the end of the next empty line where a line is terminated by the
/// exact sequence of bytes b"\r\n".
///
/// NOTE: This assumes that the input bytes re
pub struct LineMatcher {
    seen_cr: bool,

    /// Number of bytes seen in the current line.
    cur_length: usize,

    empty_only: bool,
}

impl LineMatcher {
    pub fn any() -> Self {
        LineMatcher {
            seen_cr: false,
            cur_length: 0,
            empty_only: false,
        }
    }

    pub fn empty() -> Self {
        LineMatcher {
            seen_cr: false,
            cur_length: 0,
            empty_only: true,
        }
    }
}

impl Matcher for LineMatcher {
    fn process(&mut self, data: &[u8]) -> Option<usize> {
        for (i, v) in data.iter().enumerate() {
            self.cur_length += 1;
            if self.seen_cr {
                self.seen_cr = false;
                if *v == ('\n' as u8) {
                    let len = self.cur_length;
                    self.cur_length = 0;
                    if len == 2 || !self.empty_only {
                        return Some(i + 1);
                    }
                }
            }
            if !self.seen_cr {
                if *v == ('\r' as u8) {
                    self.seen_cr = true;
                }
            }
        }

        None
    }
}

/*
    Buffer a lot.

*/

/// Wrapper around a byte stream which makes pattern matching more efficient.
///
/// See the read_matching() method which implements buffered reading from the
/// underlying stream to efficiently detect patterns with unknown byte offsets.
/// After it is called, the reader can continue to be used as a Readable as if
/// the pattern was read exactly without overreading.
pub struct PatternReader {
    reader: Box<dyn Readable>,

    /// TODO: Use something lighter weight like the BufferQueue.
    head: Bytes,

    options: StreamBufferOptions,
}

impl PatternReader {
    pub fn new(reader: Box<dyn Readable>, options: StreamBufferOptions) -> Self {
        PatternReader {
            reader,
            head: Bytes::new(),
            options,
        }
    }

    /// Read from the underlying stream until a match is found.
    ///
    /// TODO: If this ends up being called too many times, then the total amount
    /// of memory used by a single connection may get very large.
    pub async fn read_matching<M: Matcher>(&mut self, mut matcher: M) -> Result<StreamReadUntil> {
        let mut buf = vec![];
        buf.resize(self.options.buffer_size, 0u8);

        // Index up to which we have read.
        let mut idx = 0;

        // Index of the first match.
        let match_idx;

        // Read until we see the pattern or overflow our buffer limit.
        loop {
            let next_idx = if self.head.len() > 0 {
                if self.head.len() > self.options.max_buffer_size {
                    return Err(err_msg("No much data remaining from last run"));
                }

                assert_eq!(idx, 0);
                let n = self.head.len();
                buf.resize(std::cmp::max(buf.len(), n), 0);
                buf[0..self.head.len()].copy_from_slice(&self.head);
                self.head = Bytes::new();
                n
            } else {
                // TODO: Validate that this buffer slice is not empty.
                let nread = self.reader.read(&mut buf[idx..]).await?;
                if nread == 0 {
                    if idx != 0 {
                        // TODO: In this case, this is too much to return?
                        return Ok(StreamReadUntil::Incomplete(buf.into()));
                    } else {
                        return Ok(StreamReadUntil::EndOfStream);
                    }
                }

                idx + nread
            };

            // Try to find a match.
            let m = matcher.process(&buf[idx..next_idx]);
            idx = next_idx;

            if let Some(i) = m {
                match_idx = i;
                break;
            }

            if buf.len() - idx < self.options.buffer_size {
                let num_to_add = std::cmp::min(
                    self.options.buffer_size,
                    self.options.max_buffer_size - buf.len(),
                );

                if num_to_add == 0 {
                    return Ok(StreamReadUntil::TooLarge);
                }

                let new_size = buf.len() + num_to_add;
                buf.resize(new_size, 0u8);
            }
        }

        buf.resize(idx, 0);
        let b = Bytes::from(buf);

        // TODO: This can be more efficient with split_off
        self.head = b.slice(match_idx..);

        Ok(StreamReadUntil::Value(b.slice(0..match_idx)))
    }
}

#[async_trait]
impl Readable for PatternReader {
    async fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        let mut rest = buf;
        let mut total_read = 0;

        if self.head.len() > 0 {
            let n = std::cmp::min(rest.len(), self.head.len());
            rest[0..n].copy_from_slice(&self.head[0..n]);
            rest = &mut rest[n..];
            self.head = self.head.slice(n..);
            total_read += n;
        }

        if rest.len() > 0 {
            let nread = self.reader.read(rest).await?;
            total_read += nread;
        }

        Ok(total_read)
    }
}

pub struct StreamBufferOptions {
    /// Number of bytes we will try to read in each step of internal algorithms.
    pub buffer_size: usize,

    /// Maximum size of all buffered data.
    pub max_buffer_size: usize,
}

impl StreamBufferOptions {
    pub fn default() -> Self {
        Self {
            buffer_size: 1024,
            max_buffer_size: 16 * 1024, // 16KB
        }
    }
}

pub enum StreamReadUntil {
    /// The first item will be the bytes up to and including the delimiter.
    /// The second item will be
    Value(Bytes),

    /// Error: We have hit our memory buffer size limit before getting a match.
    TooLarge,

    ///
    Incomplete(Bytes),

    EndOfStream,
}
