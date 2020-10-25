use common::async_std::io::Read;
use common::bytes::Bytes;
use common::errors::*;
use common::futures::io::{AsyncRead, AsyncWrite};
use common::io::*;
use std::convert::AsRef;
use std::marker::Unpin;
use std::pin::Pin;
use std::sync::Arc;
use std::task::Poll;

pub trait Matcher {
    /// Considering all previous data passed to this function, try to find a
    /// match given a new chunk of data returning the index immediately after
    /// the first match.
    fn process(&mut self, data: &[u8]) -> Option<usize>;
}

// TODO: Move this somewhere else as this is very specific to http.
/// Matches the end of the next empty line where a line is terminated by the
/// exact sequence '\r\n'.
pub struct LineMatcher {
    seen_cr: bool,
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

// TODO: Rename StreamIO as this now supports passing through write as well
pub struct StreamReader {
    reader: Arc<dyn Readable>,
    head: Bytes,
}

// pub struct StreamBufferOptions {
// 	buffer_size: usize,
// 	max_buffer_size: usize
// }

pub enum StreamReadUntil {
    /// The first item will be the bytes up to and including the delimiter.
    /// The second item will be
    Value(Bytes),
    TooLarge,
    Incomplete(Bytes),
    EndOfStream,
}

const BUFFER_SIZE: usize = 1024;
const MAX_BUFFER_SIZE: usize = 16 * 1024; // 16KB

/// A trait for any type that can be referenced as an AsyncRead.
// trait AsRead {
// 	type Inner;
// 	fn as_read(&self) -> &Self::Inner;
// }

// // TODO: Generalize this for non-Arc types?
// impl<T> AsRead for Arc<T> where for<'a> &'a T: AsyncRead + Unpin {
// 	type Inner = T;
// 	fn as_read(&self) -> &T {
// 		self.as_ref()
// 	}
// }

// impl<R> StreamReader<R> where R: AsRead, for<'a> &'a R::Inner: AsyncRead +
// Unpin  {
impl StreamReader {
    pub fn new(reader: Arc<dyn Readable>) -> Self {
        StreamReader {
            reader,
            head: Bytes::new(),
        }
    }

    // TODO: If this ends up being called too many times, then the total amount of
    // memory used by a single connection may get very large.
    pub async fn read_matching<M: Matcher>(&mut self, mut matcher: M) -> Result<StreamReadUntil> {
        let mut buf = vec![];
        buf.resize(BUFFER_SIZE, 0u8);

        // Index up to which we have read.
        let mut idx = 0;

        // Index of the first match.
        let match_idx;

        // Read until we see the pattern or overflow our buffer limit.
        loop {
            let next_idx = if self.head.len() > 0 {
                if self.head.len() > MAX_BUFFER_SIZE {
                    return Err(err_msg("No much data remaining from last run"));
                }

                assert_eq!(idx, 0);
                let n = self.head.len();
                buf.resize(std::cmp::max(buf.len(), n), 0);
                buf[0..self.head.len()].copy_from_slice(&self.head);
                self.head = Bytes::new();
                n
            } else {
                let r = self.reader.as_ref();
                let nread = r.read(&mut buf[idx..]).await?;
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

            if buf.len() - idx < BUFFER_SIZE {
                let num_to_add = std::cmp::min(BUFFER_SIZE, MAX_BUFFER_SIZE - buf.len());

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

    pub async fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
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
            let mut r = self.reader.as_ref();
            let nread = r.read(rest).await?;
            total_read += nread;
        }

        Ok(total_read)
    }

    pub async fn read_exact(&mut self, mut buf: &mut [u8]) -> Result<()> {
        while buf.len() > 0 {
            let n = self.read(buf).await?;
            buf = &mut buf[n..];
        }

        Ok(())
    }
}

// impl AsyncWrite for StreamReader {
// 	fn poll_write(
//         self: Pin<&mut Self>,
//         cx: &mut std::task::Context,
//         buf: &[u8]
//     ) -> Poll<std::io::Result<usize>> {
// 		let mut r = &*self.reader;
// 		r.poll_write(cx, buf)
// 	}

// 	// fn poll_flush(
//     //     self: Pin<&mut Self>,
//     //     cx: &mut std::task::Context
//     // ) -> Poll<std::io::Result<()>> {
// 	// 	self.reader.poll_flush(cx)
// 	// }
//     // fn poll_close(
//     //     self: Pin<&mut Self>,
//     //     cx: &mut std::task::Context
//     // ) -> Poll<std::io::Result<()>> {
// 	// 	let a: Arc<TcpStream> = self.reader.clone();
// 	// 	let mut r: &dyn AsyncWrite = &*a;
// 	// 	r.write_all(b"hello");
// 	// 	(*r).poll_close(cx)
// 	// }
// }

// / Wrapper around StreamReader for having multiple copies
// pub struct SharedStreamReader<R> {
// 	stream: Arc<Mutex<StreamReader<R>>>
// }

// impl<R> SharedStreamReader<R> {
// 	pub fn new(stream: StreamReader<R>) -> Self {
// 		SharedStreamReader { stream: Arc::new(Mutex::new(stream)) }
// 	}
// }

// impl<R: AsyncRead> SharedStreamReader<R> {
// 	pub fn read(&mut self, buf: &mut [u8]) -> impl common::FutureResult<usize> {
// 		self.stream.lock().unwrap().read(buf)
// 	}

// 	pub fn read_exact(&mut self, mut buf: &mut [u8]) -> impl
// common::FutureResult<()> { 		self.stream.lock().unwrap().read_exact(buf)
// 	}
// }

// impl<R: AsyncWrite> AsyncWrite for SharedStreamReader<R> {
// 	fn poll_write(
//         self: Pin<&mut Self>,
//         cx: &mut std::task::Context,
//         buf: &[u8]
//     ) -> Poll<std::io::Result<usize>> {
// 		self.stream.lock().unwrap().poll_write(cx, buf)
// 	}
// }
