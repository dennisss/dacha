use common::async_std::net::TcpStream;
use common::bytes::Bytes;
use common::errors::*;
use crate::reader::*;
use std::sync::Arc;

// Marker that indicates the end of the HTTP headers.
const HTTP_MESSAGE_ENDMARKER: &'static [u8] = b"\r\n\r\n";

// TODO: Pass these into the read_matching()
// const buffer_size: usize = 1024;

// If we average an http preamable (request line + headers) larger than this size, then we will fail the request.
// const max_buffer_size: usize = 16*1024; // 16KB


pub enum HttpStreamEvent {
	/// Read the entire head of a http request/response message.
	/// The raw bytes of the head will be the first item of this tuple.
	/// All remaining bytes after the head are stored in the second item.
	MessageHead(Bytes),

	/// The size of the message status line + headers is too large to fit into the internal buffers.
	HeadersTooLarge,

	/// The end of the TCP stream was hit, but there are left over bytes that don't represent a complete message. 
	Incomplete(Bytes),

	/// The end of the Tcp stream was hit without any other messages being read.
	EndOfStream
}

pub async fn read_http_message<'a>(
	stream: &mut StreamReader)
-> Result<HttpStreamEvent> {

	// TODO: When we get the *first* CRLF, check if we got a 0.9 request
	// ^ TODO: Should only do the above when processing a request (it would be invalid during a response).

	let val = stream.read_matching(LineMatcher::empty()).await?;
	Ok(match val {
		StreamReadUntil::Value(buf) => HttpStreamEvent::MessageHead(buf),
		StreamReadUntil::TooLarge => HttpStreamEvent::HeadersTooLarge,
		StreamReadUntil::Incomplete(b) => HttpStreamEvent::Incomplete(b),
		StreamReadUntil::EndOfStream => HttpStreamEvent::EndOfStream
	})
}
