use std::io::{Read, Write};
use std::sync::Arc;
use std::io::{Cursor};
use std::sync::mpsc;
use std::convert::TryFrom;
use std::borrow::{BorrowMut, Borrow};
use std::future::Future;
use std::marker::{Unpin};
use std::pin::Pin;
use common::bytes::Bytes;
use common::futures::io::AsyncRead;
use common::errors::*;
use common::FutureResult;
use crate::reader::*;

pub type BoxFutureResult<'a, T> = Pin<Box<dyn FutureResult<T> + Send + 'a>>;

// TODO: Merge with Readable trait?
#[async_trait]
pub trait Body: Send + Sync {
	/// Returns the total length in bytes of the body payload. Will return None
	/// if the length is unknown without reading the entire body.
	/// 
	/// NOTE: This is only guaranteed to be valid before read() is called.
	fn len(&self) -> Option<usize>;

	async fn read(&mut self, buf: &mut [u8]) -> Result<usize>;

	async fn read_to_end(&mut self, buf: &mut Vec<u8>) -> Result<()> {
		let mut i = buf.len();
		loop {
			buf.resize(i + BUF_SIZE, 0);

			let res = self.read(&mut buf[i..]).await;
			match res {
				Ok(n) => {
					i += n;
					if n == 0 {
						buf.resize(i, 0);
						return Ok(());
					}
				},
				Err(e) => {
					buf.resize(i, 0);
					return Err(e);
				}
			}
		}
	}
}

const BUF_SIZE: usize = 4096;

// TODO: Add a generic trait for allowing using a future impl for Read?
impl dyn Body + Send + Sync {


	pub async fn read_exact(&mut self, mut buf: &mut [u8]) -> Result<()> {
		while buf.len() > 0 {
			let n = self.read(buf).await?;
			buf = &mut buf[n..];
		}

		Ok(())
	}
}

/*
	In the response, If I have a 
*/

#[async_trait]
impl<T: AsRef<[u8]> + Send + Sync> Body for Cursor<T> {
	fn len(&self) -> Option<usize> {
		Some(self.get_ref().as_ref().len())
	}

	async fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
		std::io::Read::read(self, buf).map_err(|e| Error::from(e))
	}
}


// pub struct BodyAsyncRead<'a> {
// 	fut: Pin<Box<dyn FutureResult<usize> + Send + 'a>>
// }
// use std::poll::Poll;
// use std::task::Context;
// impl<'a> AsyncRead for BodyAsyncRead<'a> {
// 	fn poll_read(
// 		self: Pin<&mut Self>, 
// 		cx: &mut Context, 
// 		buf: &mut [u8]
// 	) -> std::poll::Poll<std::io::Result<usize, Error>> {
// 		self.fut.poll
// 	}
// }





pub fn EmptyBody() -> Box<dyn Body> {
	Box::new(Cursor::new(Vec::new()))
}

pub fn BodyFromData<T: 'static + AsRef<[u8]> + Send + Sync>(data: T)
	-> Box<dyn Body> {
	Box::new(Cursor::new(data))
}

// TODO: HTTP/1.0 clients should not be assumes to support chunked encoding

// TODO: Any response to a HEAD request is always an empty body (headers should not be interpreted)

// pub struct OutgoingBody {
// 	pub stream: TcpStream
// }

// impl Write for OutgoingBody {
// 	fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
// 		let mut guard = self.stream.lock().unwrap();
// 		let s: &mut TcpStream = guard.borrow_mut();
// 		s.write(buf)
// 	}
// 	fn flush(&mut self) -> std::io::Result<()> {
// 		let mut guard = self.stream.lock().unwrap();
// 		let s: &mut TcpStream = guard.borrow_mut();
// 		s.flush()
// 	}
// }

pub enum Chunk {
	Data(Bytes),
	End
}

pub type ChunkSender = mpsc::Sender<Chunk>;

/// A body that gets incrementally sent over the wire and receives whole chunks from a 
/// TODO: Need flow control
pub struct ChunkedBody {
	receiver: mpsc::Receiver<Chunk>,
	
	/// Last chunk that we have received over the channel.
	chunk: Option<Chunk>
}

impl ChunkedBody {
	pub fn new() -> (Self, ChunkSender) {
		let (send, recv) = mpsc::channel();
		let c = ChunkedBody {
			receiver: recv,
			chunk: None
		};

		(c, send)
	}
}

/// A body which is terminated by the end of the stream and has no known length.
pub struct IncomingUnboundedBody {
	pub stream: StreamReader
}

#[async_trait]
impl Body for IncomingUnboundedBody {
	fn len(&self) -> Option<usize> {
		None
	}

	async fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
		self.stream.read(buf).await
	}
}

/// A body which has a well known length.
pub struct IncomingSizedBody {
	pub length: usize,
	pub stream: StreamReader
}

#[async_trait]
impl Body for IncomingSizedBody {
	fn len(&self) -> Option<usize> {
		None
	}

	async fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
		if self.length == 0 || buf.len() == 0 {
			return Ok(0);
		}

		let n = std::cmp::min(self.length, buf.len());
		let nread = self.stream.read(&mut buf[0..n]).await?;
		self.length -= nread;

		if n == 0 && self.length != 0 {
			return Err(err_msg("Unexpected end to stream"));
		}

		Ok(nread)
	}
}


// TODO: Make these all private
// XXX: The important thing is that we never allow reading if we are out of sync with the underylying ReadStream
// pub struct IncomingBody {
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

// 		// TODO: Should block reading after the response has been sent (aka error out). 
		
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
				
// 				// Because there can be multiple requests per connection, we can't allow a handler to hold on to a reference to the body after the response has finished being sent.
// 				if s.idx != self.idx + self.start_idx {
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