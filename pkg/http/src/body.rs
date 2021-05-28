use std::{io::Cursor, ops::DerefMut};
use std::pin::Pin;
use std::sync::mpsc;
use std::ops::Deref;

use common::io::Readable;
use common::bytes::Bytes;
use common::errors::*;
use common::FutureResult;
use compression::transform::Transform;
use common::borrowed::Borrowed;

use crate::reader::*;
use crate::header::Headers;

pub type BoxFutureResult<'a, T> = Pin<Box<dyn FutureResult<T> + Send + 'a>>;

// TODO: Merge with Readable trait?
// TODO: Rename IncomingBody?
#[async_trait]
pub trait Body: Readable {
    /// Returns the total length in bytes of the body payload. Will return None
    /// if the length is unknown without reading the entire body.
    ///
    /// This is the actual transferred length after decoding. Some response bodies
    /// to requests such as HEAD may still have a Content-Length header while
    /// having a body.len() of 0.
    ///
    /// NOTE: This is only guaranteed to be valid before read() is called
    /// (otherwise some implementations may return the remaining length).
    fn len(&self) -> Option<usize>;

    /// Retrieves the trailer headers that follow the body (if any).
    ///
    /// This should only be called after all data has been read from the body.
    /// Otherwise, this may fail. It's also invalid to call this more than
    /// once.
    async fn trailers(&mut self) -> Result<Option<Headers>>;
}


/*
    In the response, If I have a
*/

#[async_trait]
impl<T: 'static + AsRef<[u8]> + Send + Unpin> Body for Cursor<T> {
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
pub fn BodyFromData<T: 'static + AsRef<[u8]> + Send + Unpin>(data: T) -> Box<dyn Body> {
    Box::new(Cursor::new(data))
}

// TODO: HTTP/1.0 clients should not be assumes to support chunked encoding

// TODO: Any response to a HEAD request is always an empty body (headers should
// not be interpreted)

// TODO: Move to the chunked file.
pub enum Chunk {
    Data(Bytes),
    End,
}

pub type ChunkSender = mpsc::Sender<Chunk>;


/// TODO: Implement this?
/// A body that gets incrementally sent over the wire and receives whole chunks
/// from a TODO: Need flow control
pub struct ChunkedBody {
    receiver: mpsc::Receiver<Chunk>,

    /// Last chunk that we have received over the channel.
    chunk: Option<Chunk>,
}

impl ChunkedBody {
    pub fn new() -> (Self, ChunkSender) {
        let (send, recv) = mpsc::channel();
        let c = ChunkedBody {
            receiver: recv,
            chunk: None,
        };

        (c, send)
    }
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

    async fn trailers(&mut self) -> Result<Option<Headers>> { Ok(None) }
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
            error: false
        }
    }
}

#[async_trait]
impl Body for IncomingSizedBody {
    fn len(&self) -> Option<usize> {
        None
    }

    async fn trailers(&mut self) -> Result<Option<Headers>> { Ok(None) }
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
            return Err(err_msg("Unexpected end to stream"));
        }

        Ok(nread)
    }
}

/// Body which applies a given transform to another body.
/// 
/// If this is read to the end, then it will internally ensure the entire inner body can
/// be transformed by the transform without extra bytes.
pub struct TransformBody {
    /// Input body which we are transforming.
    body: Box<dyn Body>,

    /// The transform which is being applied.
    transform: Box<dyn Transform + Send + Sync>,

    /// Data that has been read from the input body but hasn't been digested by
    /// the Transform.
    input_buffer: Vec<u8>,

    input_buffer_offset: usize,

    /// Whether or not we have read all of the input data yet.
    end_of_input: bool,

    /// Whether or not the transform is done (no more data will be outputted).
    end_of_output: bool,
}

impl TransformBody {
    pub fn new(body: Box<dyn Body>, transform: Box<dyn Transform + Send + Sync>) -> Self {
        let mut input_buffer = vec![];
        input_buffer.reserve_exact(512);

        Self { body, transform, input_buffer, input_buffer_offset: 0, end_of_input: false, end_of_output: false }
    }
}

#[async_trait]
impl Body for TransformBody {
    fn len(&self) -> Option<usize> { self.body.len() }

    async fn trailers(&mut self) -> Result<Option<Headers>> { self.body.trailers().await }
}

#[async_trait]
impl Readable for TransformBody {
    async fn read(&mut self, mut output: &mut [u8]) -> Result<usize> {
        let mut output_written = 0;
        
        loop {
            // Trivially can't do anything in this case.
            // NOTE: end_of_input will always be set after end_of_output.
            if output.is_empty() || self.end_of_input {
                return Ok(0);
            }

            if !self.input_buffer.is_empty() {
                // TODO: attempt to execute this multiple times if no data was consumed.
                let progress = self.transform.update(
                    &self.input_buffer[self.input_buffer_offset..], self.end_of_input, output)?;

                self.input_buffer_offset += progress.input_read;
                if self.input_buffer_offset == self.input_buffer.len() {
                    // All input data was consumed. Can clear the buffer.
                    self.input_buffer_offset = 0;
                    self.input_buffer.clear();
                }

                output_written += progress.output_written;
                output = &mut output[progress.output_written..];

                if progress.done {
                    self.end_of_output = true;
                    if !self.input_buffer.is_empty() {
                        return Err(err_msg("Remaining input data after end of output"));
                    }
                }

                if !self.input_buffer.is_empty() {
                    // Input data is remaining. Likely we ran out of space in the output buffer. 
                    // NOTE: We won't read new data from the input body until all current data has been consumed. 

                    if output_written == 0 {
                        return Err(err_msg("Transform made no progress"));
                    }

                    return Ok(output_written);
                }

                continue;
            }

            // Read more data into the input buffer.
            self.input_buffer.resize(self.input_buffer.capacity(), 0);
            let n = self.body.read(&mut self.input_buffer).await?;
            self.input_buffer.truncate(n);

            if n == 0 {
                self.end_of_input = true;
                if !self.end_of_output {
                    return Err(err_msg("End of input seen before end of output"));
                }

                return Ok(0);
            }

            // We now have data in our buffer which will be transformed in the next iteration of this loop. 
        }
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



// TODO: Make these all private
// XXX: The important thing is that we never allow reading if we are out of sync
// with the underylying ReadStream pub struct IncomingBody {
// 	/// Absolute index in the underlying ReadStream at which this body starts.
// 	pub start_idx: usize,

// 	// Current position relative to the start of the body (incremented on reads).
// 	pub idx: usize,
// 	// Number of bytes we expect (if a Content-Length header was given).
// 	pub length: Option<usize>,
// 	// Extra bytes already read after the end of the
// 	// TODO: This may contain extra bytes after completion for the next request.
// 	pub head: Bytes,

// 	pub stream: Arc<Mutex<StreamReader<TcpStream>>>
// }

// impl Read for IncomingBody {
// 	fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {

// 		// TODO: Should block reading after the response has been sent (aka error
// out).

// 		if Some(self.idx) == self.length {
// 			return Ok(0);
// 		}

// 		let mut rest = buf;
// 		let mut total_read = 0;

// 		// TODO: Eventually we cn drop the head bytes reference
// 		if rest.len() > 0 && self.idx < self.head.len() {
// 			let n = std::cmp::min(rest.len(), self.head.len() - self.idx);
// 			rest[0..n].copy_from_slice(&self.head[self.idx..(self.idx + n)]);
// 			total_read += n;
// 			rest = &mut rest[n..];
// 			self.idx += n;
// 		}

// 		if rest.len() > 0 && self.idx < self.head.len() {
// 			let n = if let Some(length) = self.length {
// 				std::cmp::min(rest.len(), length - self.idx)
// 			} else {
// 				rest.len()
// 			};

// 			if n > 0 {
// 				let mut s = self.stream.lock().unwrap();

// 				// Because there can be multiple requests per connection, we can't allow a
// handler to hold on to a reference to the body after the response has finished
// being sent. 				if s.idx != self.idx + self.start_idx {
// 					return Err(std::io::Error::new(std::io::ErrorKind::Other,
// 						"Reading from incoming body after response was sent"));
// 				}

// 				let nread = s.read(&mut rest[0..n])?;
// 				self.idx += nread;
// 				total_read += nread;
// 			}
// 		}

// 		Ok(total_read)
// 	}
// }
