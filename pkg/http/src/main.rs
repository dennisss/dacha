#![feature(core_intrinsics, async_await, trait_alias)]

#[macro_use] extern crate parsing;
extern crate bytes;
extern crate libc;

use common::errors::*;
use parsing::*;
use parsing::iso::*;

mod reader;
mod common_parser;
mod uri;
mod uri_parser;
mod dns;
mod status_code;
mod body;
mod spec;
mod message;
mod message_parser;
mod header_parser;
mod header;
mod client;
mod chunked;
mod transfer_encoding;

use reader::*;
use uri::*;
use uri_parser::*;
use dns::*;
use std::io;
use std::io::{Read, Write, Cursor};
use bytes::Bytes;
use std::sync::{Arc, Mutex};
use spec::*;
use status_code::*;
use std::thread;
use message::*;
use message_parser::*;
use header_parser::*;
use body::*;
use std::convert::TryFrom;
use std::str::FromStr;
use client::*;
use std::borrow::BorrowMut;
use chunked::*;
use transfer_encoding::*;
use std::convert::AsMut;
use parsing::ascii::*;

use async_std::net::{TcpListener, TcpStream};
use async_std::task;
use async_std::prelude::*;



// TODO: Pipelining?

async fn handle_client(stream: TcpStream) -> Result<()> {

	let stream = Arc::new(stream);
	let mut write_stream: &TcpStream = stream.as_ref();
	let mut read_stream = StreamReader::new(stream.clone());

	// Remaining bytes from the last request read.
	// TODO: Start using this?
	// let mut last_remaining = None;

	let head = match read_http_message(&mut read_stream).await? {
		HttpStreamEvent::MessageHead(h) => h,
		HttpStreamEvent::HeadersTooLarge => {
			write_stream.write_all(b"HTTP/1.1 431 Request Header Fields Too Large\r\n\r\n").await?;
			return Ok(());
		}
		HttpStreamEvent::EndOfStream | HttpStreamEvent::Incomplete(_) => {
			return Ok(());
		}
	};

	let msg = match parse_http_message_head(head) {
		Ok((msg, rest)) => {
			assert_eq!(rest.len(), 0);	
			msg
		},
		Err(e) => {
			println!("Failed to parse message\n{}", e);
			write_stream.write_all(b"HTTP/1.1 400 Bad Request\r\n\r\n").await?;
			return Ok(());
		}
	};

	let start_line = msg.start_line;
	let headers = msg.headers;

	// Verify that we got a Request style message
	let request_line = match start_line {
		StartLine::Request(r) => r,
		StartLine::Response(r) => {
			println!("Unexpected response: {:?}", r);
			write_stream.write_all(b"HTTP/1.1 400 Bad Request\r\n\r\n").await?;
			return Ok(());
		}
	};

	// Verify supported HTTP version
	match request_line.version {
		HTTP_V0_9 => {},
		HTTP_V1_0 => {},
		HTTP_V1_1 => {},
		// HTTP_V2_0 => {},
		_ => {
			println!("Unsupported http version: {:?}", request_line.version);
			write_stream.write_all(
				b"HTTP/1.1 505 HTTP Version Not Supported\r\n\r\n").await?;
			return Ok(())
		}
	};

	// Validate method
	let method = match Method::try_from(request_line.method.data.as_ref()) {
		Ok(m) => m,
		Err(_) => {
			println!("Unsupported http method: {:?}", request_line.method);
			write_stream.write_all(b"HTTP/1.1 405 Method Not Allowed\r\n\r\n").await?;
			return Ok(());
		}
	};

	// TODO: Extract content-length and transfer-encoding
	// ^ It would be problematic for a request/response to have both

	let content_length = match parse_content_length(&headers) {
		Ok(len) => len,
		Err(e) => {
			println!("{}", e);
			write_stream.write_all(b"HTTP/1.1 400 Bad Request\r\n\r\n").await?;
			return Ok(());
		}
	};

	println!("Content-Length: {:?}", content_length);

	// TODO: See https://tools.ietf.org/html/rfc7230#section-3.5 for robustness tips and accepting empty lines before a request-line.

	// TODO: See https://tools.ietf.org/html/rfc7230#section-3.3.3 with special HEAD/status code behavior



	// TODO: Will definately need to abstract getting a body for a request.
	let body: Box<dyn Body> = match content_length {
		Some(len) => Box::new(IncomingSizedBody {
			stream: read_stream,
			length: len
		}),
		None => Box::new(IncomingUnboundedBody {
			stream: read_stream,
		})
	};

	let req = Request {
		head: RequestHead {
			method,
			uri: request_line.target.into_uri(),
			version: request_line.version,
			headers,
		},
		body
	};

	let mut res = handle_request(req).await;

	// TODO: Don't allow headers such as 'Connection'

	// TODO: Must always send 'Date' header.
	// TODO: Add 'Server' header

	// TODO: If we do detect multiple aliases to a TcpStream, shutdown the tcpstream explicitly

	// let mut res_writer = OutgoingBody { stream: shared_stream.clone() };
	let mut buf = vec![];
	res.head.serialize(&mut buf);
	write_stream.write_all(&buf).await?;

	write_body(res.body.as_mut(), &mut write_stream).await?;

	Ok(())
}

// pub fn get_body(headers: &HttpHeaders, stream: Arc<Mutex<StreamReader<TcpStream>>>)
// 	-> Box<dyn Read> {

// }

pub fn new_header(name: String, value: String) -> HttpHeader {
	HttpHeader {
		name: unsafe { AsciiString::from_ascii_unchecked(Bytes::from(name)) },
		value: Latin1String::from_bytes(Bytes::from(value)).unwrap()
	}
}

// If we send back using a chunked encoding, 
async fn handle_request(mut req: Request) -> Response {

	println!("GOT: {:?}", req);

	let mut data = vec![];
	// req.body.read_to_end(&mut data).await;

	// println!("READ: {:?}", data);

	let res_headers = vec![
		new_header("Content-Length".to_string(), format!("{}", data.len()))
	];

	Response {
		head: ResponseHead {
			status_code: OK,
			version: HTTP_V1_1, // TODO: Always respond with version <= client version?
			reason: OK.default_reason().unwrap_or("").to_owned(),
			headers: HttpHeaders::from(res_headers)
		},
		body: BodyFromData(data)
	}
}

async fn run_server_client(stream: TcpStream) {
	match handle_client(stream).await {
		Ok(v) => {},
		Err(e) => println!("Client thread failed: {}", e)
	};
}

// Implementing stuff for Body.


use compression::gzip::*;

async fn run_server() -> Result<()> {
	// TODO: Follow redirects (301 and 302) or if Location is set

	let mut client = Client::create("http://www.google.com")?;

	let req = RequestBuilder::new()
		.method(Method::GET)
		.uri("/index.html")
		.header("Accept", "text/html")
		// .header("Accept-Encoding", "gzip")
		.header("Host", "www.google.com")
		.header("Accept-Encoding", "gzip")
		.body(EmptyBody())
		.build()?;

	let mut res = client.request(req).await?;
	println!("{:?}", res.head);

	let content_encoding = parse_content_encoding(&res.head.headers)?;
	if content_encoding.len() > 1 {
		return Err("More than one Content-Encoding not supported".into());
	}


	let mut body_buf = vec![];
	res.body.read_to_end(&mut body_buf).await?;

	let mut f = std::fs::File::create("testdata/out/response.gz")?;
	f.write_all(&body_buf)?;
	f.flush()?;

	if content_encoding.len() == 1 {
		if content_encoding[0] == "gzip" {
			let mut c = std::io::Cursor::new(&body_buf);
			let gz = read_gzip(&mut c)?;

			body_buf = gz.data;
		}
		else {
			return Err(
				format!("Unsupported content-encoding: {}", content_encoding[0]).into());
		}
	}

	// TODO: Read Content-Type to get the charset.

	println!("BODY\n{}", Latin1String::from_bytes(body_buf.into()).unwrap().to_string());

	return Ok(());

	let listener = TcpListener::bind("127.0.0.1:8000").await?;

	let mut incoming = listener.incoming();
	while let Some(stream) = incoming.next().await {
		let s = stream?;
		task::spawn(run_server_client(s));
	}

	Ok(())
}

fn main() -> Result<()> {
	task::block_on(run_server())
}

