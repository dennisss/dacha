use std::collections::VecDeque;
use std::ops::Deref;
use std::pin::Pin;
use std::{io::Cursor, ops::DerefMut};

use common::borrowed::Borrowed;
use common::bytes::{Buf, Bytes};
use common::errors::*;
use common::io::Readable;
use common::FutureResult;
use compression::readable::TransformReadable;
use compression::transform::Transform;

use crate::header::Headers;
use crate::reader::*;

pub type BoxFutureResult<'a, T> = Pin<Box<dyn FutureResult<T> + Send + 'a>>;

#[async_trait]
pub trait Body: Readable + Sync {
    /// Returns the total length in bytes of the body payload. Will return None
    /// if the length is unknown without reading the entire body.
    ///
    /// This is the actual transferred length after decoding. Some response
    /// bodies to requests such as HEAD may still have a Content-Length
    /// header while having a body.len() of 0.
    ///
    ///
    /// NOTE: This is only guaranteed to be valid before read() is called
    /// (otherwise some implementations may return the remaining length).
    fn len(&self) -> Option<usize>;

    /// Returns whether or not this body MAY have trailers.
    ///
    /// If this returns false, then trailers() may never be called or send to
    /// the remote endpoint. But, returning false does allow options to
    /// occur.
    fn has_trailers(&self) -> bool {
        false
    }

    /// Retrieves the trailer headers that follow the body (if any).
    ///
    /// This should only be called after all data has been read from the body.
    /// Otherwise, this may fail. It's also invalid to call this more than
    /// once.
    async fn trailers(&mut self) -> Result<Option<Headers>>; // { Ok(None) }
}

#[async_trait]
impl<T: 'static + AsRef<[u8]> + Send + Sync + Unpin> Body for Cursor<T> {
    fn len(&self) -> Option<usize> {
        Some(self.get_ref().as_ref().len())
    }

    async fn trailers(&mut self) -> Result<Option<Headers>> {
        Ok(None)
    }
}

/// Creates a body containing no data.
pub fn EmptyBody() -> Box<dyn Body> {
    Box::new(Cursor::new(Vec::new()))
}

/// Creates a body from a precomputed blob of data.
pub fn BodyFromData<T: 'static + AsRef<[u8]> + Send + Sync + Unpin>(data: T) -> Box<dyn Body> {
    Box::new(Cursor::new(data))
}

pub fn WithTrailers(body: Box<dyn Body>, trailers: Headers) -> Box<dyn Body> {
    Box::new(WithTrailersBody {
        body,
        trailers: Some(trailers),
    })
}

struct WithTrailersBody {
    body: Box<dyn Body>,
    trailers: Option<Headers>,
}

#[async_trait]
impl Body for WithTrailersBody {
    fn len(&self) -> Option<usize> {
        self.body.len()
    }

    fn has_trailers(&self) -> bool {
        true
    }

    // TODO: Error out if called twice.
    async fn trailers(&mut self) -> Result<Option<Headers>> {
        Ok(self.trailers.take())
    }
}

#[async_trait]
impl Readable for WithTrailersBody {
    async fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        self.body.read(buf).await
    }
}

struct PartsBody {
    parts: VecDeque<Bytes>,
}

#[async_trait]
impl Body for PartsBody {
    fn len(&self) -> Option<usize> {
        let mut total = 0;
        for part in &self.parts {
            total += part.len();
        }

        Some(total)
    }

    async fn trailers(&mut self) -> Result<Option<Headers>> {
        Ok(None)
    }
}

#[async_trait]
impl Readable for PartsBody {
    async fn read(&mut self, mut buf: &mut [u8]) -> Result<usize> {
        let mut nread = 0;
        while buf.len() > 0 {
            let part = match self.parts.get_mut(0) {
                Some(v) => v,
                None => {
                    break;
                }
            };

            if part.len() == 0 {
                self.parts.pop_front();
                continue;
            }

            let n = std::cmp::min(buf.len(), part.len());
            (&mut buf[0..n]).copy_from_slice(&part[0..n]);
            nread += n;

            buf = &mut buf[n..];
            part.advance(n);
        }

        Ok(nread)
    }
}

pub fn BodyFromParts<I: Iterator<Item = Bytes>>(parts: I) -> Box<dyn Body> {
    Box::new(PartsBody {
        parts: parts.collect(),
    })
}

/// A body which is terminated by the end of the stream and has no known length.
pub struct IncomingUnboundedBody {
    // TODO: This doesn't need to be borrowed. It mainly is in order to simplify things.
    reader: Borrowed<PatternReader>,
}

impl IncomingUnboundedBody {
    pub fn new(reader: Borrowed<PatternReader>) -> Self {
        Self { reader }
    }
}

#[async_trait]
impl Body for IncomingUnboundedBody {
    fn len(&self) -> Option<usize> {
        None
    }

    async fn trailers(&mut self) -> Result<Option<Headers>> {
        Ok(None)
    }
}

#[async_trait]
impl Readable for IncomingUnboundedBody {
    async fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        self.reader.read(buf).await
    }
}

/// A body which has a well known length.
pub struct IncomingSizedBody {
    length: usize,
    error: bool,
    reader: Borrowed<PatternReader>, // TODO: Use a generic instead? (just needs to be 'Readable')
}

impl IncomingSizedBody {
    pub fn new(reader: Borrowed<PatternReader>, length: usize) -> Self {
        Self {
            length,
            reader,
            error: false,
        }
    }
}

#[async_trait]
impl Body for IncomingSizedBody {
    fn len(&self) -> Option<usize> {
        None
    }

    async fn trailers(&mut self) -> Result<Option<Headers>> {
        Ok(None)
    }
}

#[async_trait]
impl Readable for IncomingSizedBody {
    async fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        if self.error {
            return Err(err_msg("Body has previously failed while being read"));
        }

        if self.length == 0 || buf.len() == 0 {
            return Ok(0);
        }

        let n = std::cmp::min(self.length, buf.len());
        let nread = match self.reader.read(&mut buf[0..n]).await {
            Ok(n) => n,
            Err(e) => {
                self.error = true;
                return Err(e);
            }
        };

        self.length -= nread;

        if n == 0 && self.length != 0 {
            self.error = true;
            // TODO: This should trigger a client error to be returned (maybe use a
            // ProtocolError)
            return Err(err_msg("Unexpected end to stream"));
        }

        Ok(nread)
    }
}

/// Body which applies a given transform to another body.
///
/// If this is read to the end, then it will internally ensure the entire inner
/// body can be transformed by the transform without extra bytes.
///
/// TODO: Move this to the compression package as most of this is generic
/// readable logic.
pub struct TransformBody {
    body: TransformReadable<Box<dyn Body>>,
}

impl TransformBody {
    pub fn new(body: Box<dyn Body>, transform: Box<dyn Transform + Send + Sync>) -> Self {
        Self {
            body: TransformReadable::new(body, transform),
        }
    }
}

#[async_trait]
impl Body for TransformBody {
    fn len(&self) -> Option<usize> {
        self.body.inner_reader().len()
    }

    async fn trailers(&mut self) -> Result<Option<Headers>> {
        self.body.inner_reader_mut().trailers().await
    }
}

#[async_trait]
impl Readable for TransformBody {
    async fn read(&mut self, mut output: &mut [u8]) -> Result<usize> {
        self.body.read(output).await
    }
}

#[async_trait]
impl Body for Borrowed<Box<dyn Body>> {
    fn len(&self) -> Option<usize> {
        self.deref().len()
    }

    async fn trailers(&mut self) -> Result<Option<Headers>> {
        self.deref_mut().trailers().await
    }
}
