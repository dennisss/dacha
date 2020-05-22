use common::errors::*;
use std::cmp::min;
use std::io::Read;

/// Represents a synchronous stream interface similar in purpose to the futures
/// main
pub trait Stream {
    /// NOTE: The 'max' argument is voluntary and is mainly meant to guide
    /// blocking reads as the consumer will be totally happy with stopping as
    /// soon as it is done
    fn next(&mut self, max: usize) -> Result<Option<&[u8]>>;
}

// Ideally I do need to gurantee

/// A stream consisting of a single in-memory source
pub struct SingleStream<'a> {
    done: bool,
    data: &'a [u8],
}

impl<'a> SingleStream<'a> {
    pub fn from(data: &'a [u8]) -> SingleStream<'a> {
        SingleStream { done: false, data }
    }
}

impl<'a> Stream for SingleStream<'a> {
    fn next(&mut self, max: usize) -> Result<Option<&[u8]>> {
        Ok(if self.done { None } else { Some(&self.data) })
    }
}

/// A stream consisting of multiple existing byte buffers
pub struct ChunkedStream<'a> {
    idx: usize,
    chunks: &'a [bytes::Bytes],
}

// Much easier to define a bound for it right?
impl<'a> ChunkedStream<'a> {
    pub fn from(chunks: &'a [bytes::Bytes]) -> ChunkedStream<'a> {
        ChunkedStream { idx: 0, chunks }
    }
}

impl<'a> Stream for ChunkedStream<'a> {
    fn next(&mut self, _max: usize) -> Result<Option<&[u8]>> {
        if self.idx < self.chunks.len() {
            let idx = self.idx;
            self.idx = idx + 1;
            Ok(Some(&self.chunks[idx]))
        } else {
            Ok(None)
        }
    }
}

/// A stream derived from a Read source
pub struct ReadStream<'a> {
    readable: &'a mut Read,
    buf: [u8; READ_BUF_SIZE],
}

impl<'a> ReadStream<'a> {
    pub fn from(readable: &'a mut Read) -> ReadStream {
        ReadStream {
            readable,
            buf: [0u8; READ_BUF_SIZE],
        }
    }
}

const READ_BUF_SIZE: usize = 8*1024 /* io::DEFAULT_BUF_SIZE */;

impl<'a> Stream for ReadStream<'a> {
    // The only way this could be better is if we
    fn next(&mut self, max: usize) -> Result<Option<&[u8]>> {
        // Main issue being that it is totally limit to error out
        let m = min(self.buf.len(), max);
        let n = self.readable.read(&mut self.buf[..m])?;

        Ok(Some(&self.buf[..n]))
    }
}
