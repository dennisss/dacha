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

/// Matches the end of the next line where a line is terminated by the
/// exact sequence of bytes b"\r\n".
///
/// NOTE: This assumes that the input bytes re
///
/// TODO: Move this somewhere else as this is very specific to http.
pub struct LineMatcher {
    seen_cr: bool,

    /// Number of bytes seen in the current line.
    cur_length: usize,

    /// If true, we will only consider a match to be found once we see an empty
    /// line.
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

/// Wrapper around a byte stream which makes pattern matching more efficient.
///
/// See the read_matching() method which implements buffered reading from the
/// underlying stream to efficiently detect patterns with unknown byte offsets.
/// After it is called, the reader can continue to be used as a Readable as if
/// the pattern was read exactly without overreading.
pub struct PatternReader {
    reader: Box<dyn SharedReadable>,

    /// TODO: Use something lighter weight like the BufferQueue.
    head: Bytes,

    options: StreamBufferOptions,
}

impl PatternReader {
    pub fn new(reader: Box<dyn SharedReadable>, options: StreamBufferOptions) -> Self {
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
            let last_idx = idx;
            let m = matcher.process(&buf[idx..next_idx]);
            idx = next_idx;

            if let Some(i) = m {
                match_idx = last_idx + i;
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

#[derive(Debug, PartialEq)]
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

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use crate::message::MESSAGE_HEAD_BUFFER_OPTIONS;

    use super::*;

    #[testcase]
    async fn read_entire_message_header() -> Result<()> {
        // Pulled from requesting 'GET http://www.google.com/'
        let data = "HTTP/1.1 200 OK\r\nDate: Sat, 03 Feb 2024 20:40:30 GMT\r\nExpires: -1\r\nCache-Control: private, max-age=0\r\nContent-Type: text/html; charset=ISO-8859-1\r\nContent-Security-Policy-Report-Only: object-src 'none';base-uri 'self';script-src 'nonce-0811111kabCu_l35679eqB' 'strict-dynamic' 'report-sample' 'unsafe-eval' 'unsafe-inline' https: http:;report-uri https://csp.withgoogle.com/csp/gws/other-hp\r\nP3P: CP=\"This is not a P3P policy! See g.co/p3phelp for more info.\"\r\nServer: gws\r\nX-XSS-Protection: 0\r\nX-Frame-Options: SAMEORIGIN\r\nSet-Cookie: 1P_JAR=2024-02-03-20; expires=Mon, 04-Mar-2024 20:40:30 GMT; path=/; domain=.google.com; Secure\r\nSet-Cookie: AEC=Ae3NU9Pkzkno7FZuzF19D8KpFd4RyRfPyYKQN6tRaN805nwoWe_LgsULzA; expires=Thu, 01-Aug-2024 20:40:30 GMT; path=/; domain=.google.com; Secure; HttpOnly; SameSite=lax\r\nSet-Cookie: NID=511=abcdefghijklmnopqrstuv12-A12345677BCcBiaLetX9ra1237o7IypG12gSq9q-1T6S02nJWC8WlpaSd2_ty2afOL-AAAAABBBBBCCCCCDDDDDEEEEEFFFFFGGGGGRRRRRXmV8p-RydzXeo55555GqUYVUFj2zUpicIZXo-UQ; expires=Sun, 04-Aug-2024 20:40:30 GMT; path=/; domain=.google.com; HttpOnly\r\nAccept-Ranges: none\r\nVary: Accept-Encoding\r\nTransfer-Encoding: chunked\r\n\r\n34de\r\n<!doctype html";

        let mut reader = PatternReader::new(
            Box::new(Cursor::new(data.as_bytes().to_vec())),
            MESSAGE_HEAD_BUFFER_OPTIONS,
        );

        let res = reader.read_matching(LineMatcher::empty()).await?;

        assert_eq!(res, StreamReadUntil::Value(Bytes::from(&b"HTTP/1.1 200 OK\r\nDate: Sat, 03 Feb 2024 20:40:30 GMT\r\nExpires: -1\r\nCache-Control: private, max-age=0\r\nContent-Type: text/html; charset=ISO-8859-1\r\nContent-Security-Policy-Report-Only: object-src 'none';base-uri 'self';script-src 'nonce-0811111kabCu_l35679eqB' 'strict-dynamic' 'report-sample' 'unsafe-eval' 'unsafe-inline' https: http:;report-uri https://csp.withgoogle.com/csp/gws/other-hp\r\nP3P: CP=\"This is not a P3P policy! See g.co/p3phelp for more info.\"\r\nServer: gws\r\nX-XSS-Protection: 0\r\nX-Frame-Options: SAMEORIGIN\r\nSet-Cookie: 1P_JAR=2024-02-03-20; expires=Mon, 04-Mar-2024 20:40:30 GMT; path=/; domain=.google.com; Secure\r\nSet-Cookie: AEC=Ae3NU9Pkzkno7FZuzF19D8KpFd4RyRfPyYKQN6tRaN805nwoWe_LgsULzA; expires=Thu, 01-Aug-2024 20:40:30 GMT; path=/; domain=.google.com; Secure; HttpOnly; SameSite=lax\r\nSet-Cookie: NID=511=abcdefghijklmnopqrstuv12-A12345677BCcBiaLetX9ra1237o7IypG12gSq9q-1T6S02nJWC8WlpaSd2_ty2afOL-AAAAABBBBBCCCCCDDDDDEEEEEFFFFFGGGGGRRRRRXmV8p-RydzXeo55555GqUYVUFj2zUpicIZXo-UQ; expires=Sun, 04-Aug-2024 20:40:30 GMT; path=/; domain=.google.com; HttpOnly\r\nAccept-Ranges: none\r\nVary: Accept-Encoding\r\nTransfer-Encoding: chunked\r\n\r\n"[..])));

        let mut rest = vec![];
        reader.read_to_end(&mut rest).await?;

        assert_eq!(&rest[..], b"34de\r\n<!doctype html");

        Ok(())
    }
}
