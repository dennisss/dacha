
use std::io::Read;
use bytes::Bytes;
use std::sync::{Arc, Mutex};
use crate::reader::*;
use crate::common_parser::*;
use async_std::net::TcpStream;
use parsing::*;
use parsing::iso::*;
use common::errors::*;
use common::FutureResult;
use crate::ascii::*;
use crate::message_parser::*;
use crate::spec::*;
use std::future::Future;
use crate::body::{Body, BoxFutureResult};
use std::pin::Pin;

struct ChunkExtension {
	name: AsciiString,
	value: Option<Latin1String>
}

struct ChunkHead {
	size: usize,
	extensions: Vec<ChunkExtension>
}

// `chunk-size = 1*HEXDIG`
// TODO: Ensure not out of range.
parser!(parse_chunk_size<usize> => {
	map(take_while1(|i| (i as char).is_digit(16)),
		|data| usize::from_str_radix(
			std::str::from_utf8(&data).unwrap(), 16).unwrap())
});

// chunk-ext = *( ";" chunk-ext-name [ "=" chunk-ext-val ] )
parser!(parse_chunk_ext<Vec<ChunkExtension>> => {
	many(seq!(c => {
		c.next(one_of(";"))?;
		let name = c.next(parse_chunk_ext_name)?;
		let value = c.next(opt(seq!(c => {
			c.next(one_of("="))?;
			Ok(c.next(parse_chunk_ext_val)?)
		})))?;

		Ok(ChunkExtension { name, value })
	}))
});

// `chunk-ext-name = token`
parser!(parse_chunk_ext_name<AsciiString> => parse_token);

// `chunk-ext-val = token / quoted-string`
parser!(parse_chunk_ext_val<Latin1String> => alt!(
	parse_quoted_string,
	and_then(parse_token, |s| Latin1String::from_bytes(s.data))
));

// `= chunk-size [ chunk-ext ] CRLF`
parser!(parse_chunk_start<ChunkHead> => seq!(c => {
	let size = c.next(parse_chunk_size)?;
	let extensions = c.next(opt(parse_chunk_ext))?
		.unwrap_or(vec![]);
	c.next(parse_crlf)?;

	Ok(ChunkHead { size, extensions })
}));

// `= trailer-part CRLF`
parser!(parse_chunk_end<Vec<HttpHeader>> => {
	seq!(c => {
		let headers = c.next(parse_trailer_part)?;
		c.next(parse_crlf)?;
		Ok(headers)
	})
});

// `trailer-part = *( header-field CRLF )`
parser!(parse_trailer_part<Vec<HttpHeader>> => {
	many(parse_header_field)
});

#[derive(Clone)]
enum ChunkState {
	/// Reading the first line of the chunk containing the size
	Start,
	/// Reading the data in the chunk
	Data(usize),
	/// Done reading the data in the chunk and reading the empty line endings immediately after the data.
	End,
	/// Reading the final trailer of body until 
	Trailer,
	/// The entire body has been read.
	Done
}


pub struct IncomingChunkedBody {
	stream: StreamReader,
	state: ChunkState
}

enum CycleValue {
	StateChange,
	Read(usize)
}

impl IncomingChunkedBody {
	pub fn new(stream: StreamReader) -> Self {
		IncomingChunkedBody { stream, state: ChunkState::Start }
	}

	// TODO: Once an error occurs, then all sequential reads should also error out.
	async fn read_cycle(&mut self, buf: &mut [u8]) -> Result<CycleValue> {

		match self.state.clone() {
			ChunkState::Start => {
				let line = match self.stream.read_matching(LineMatcher::any()).await {
					Ok(StreamReadUntil::Value(v)) => v,
					Err(_) => { return Err("IO error while reading chunk start line".into()); },
					_ => { return Err("Expected chunk start line".into()); }
				};

				let (head, _) = match complete(parse_chunk_start)(line) {
					Ok(v) => v,
					_ => { return Err("Invalid chunk start line".into()); }
				};

				// TODO: Do something with the extensions

				if head.size == 0 {
					self.state = ChunkState::Trailer;
				} else {
					self.state = ChunkState::Data(head.size);
				}

				Ok(CycleValue::StateChange)
			},
			ChunkState::Data(len) =>  {
				// TODO: Also try reading = \r\n in the same call?
				let n = std::cmp::min(len, buf.len());
				let nread = self.stream.read(&mut buf[0..n]).await?;
				if nread == 0 && buf.len() > 0 {
					return Err("Reached end of stream before end of data".into());
				}

				let new_len = len - nread;
				if new_len == 0 {
					self.state = ChunkState::End;
				} else {
					self.state = ChunkState::Data(new_len);
				}

				Ok(CycleValue::Read(nread))
			},
			ChunkState::End => {
				let mut buf = [0u8; 2];
				self.stream.read_exact(&mut buf).await?;
				if &buf != b"\r\n" {
					return Err("Expected CRLF after chunk data".into());
				}

				self.state = ChunkState::Start;

				Ok(CycleValue::StateChange)
			},
			ChunkState::Trailer => {
				let data = match self.stream.read_matching(LineMatcher::empty()).await {
					Ok(StreamReadUntil::Value(v)) => v,
					Err(_) => { return Err("io error while reading trailer".into()); },
					_ => { return Err("Expected trailer empty line".into()); }
				};

				let (headers, _) = match complete(parse_chunk_end)(data) {
					Ok(v) => v,
					_ => { return Err("Invalid chunk trailer".into()); }
				};

				// TODO: Do something with the headers

				self.state = ChunkState::Done;

				Ok(CycleValue::StateChange)
			},
			ChunkState::Done => {
				Ok(CycleValue::Read(0))
			}
		}
	}

	/// Unboxed implementation of the Body::read method.
	async fn read_impl(&mut self, buf: &mut [u8]) -> Result<usize> {
		// TODO: Have a solution that doesn't require a loop here.
		loop {
			match self.read_cycle(buf).await? {
				CycleValue::Read(n) => {
					return Ok(n);
				},
				CycleValue::StateChange => {}
			}
		}
	}

}

impl Body for IncomingChunkedBody {
	fn read<'a>(&'a mut self, mut buf: &'a mut [u8])
	-> BoxFutureResult<'a, usize> {
		Box::pin(self.read_impl(buf))
	}

	fn len(&self) -> Option<usize> {
		None
	}
}


#[cfg(test)]
mod tests {
	use super::*;
	use std::io::Cursor;

	const TEST_BODY: &'static [u8] =
		b"7\r\nMozilla\r\n9\r\nDeveloper\r\n7\r\nNetwork\r\n0\r\n\r\n";

	#[test]
	fn chunked_body_test() {
		let data = Cursor::new(TEST_BODY);
		let stream = StreamReader::new(data);
		let mut body = IncomingChunkedBody::new(stream);

		let mut outbuf = String::new();
		body.read_to_string(&mut outbuf).unwrap();

		assert_eq!(&outbuf, "MozillaDeveloperNetwork");
	}
}
