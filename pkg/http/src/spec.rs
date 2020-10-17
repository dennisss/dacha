
use std::marker::Unpin;
use common::bytes::Bytes;
use common::async_std::io::Write;
use common::futures::io::AsyncWrite;
use common::errors::*;
use common::io::*;
use parsing::iso::*;
use parsing::ascii::*;
use parsing::complete;
use crate::status_code::*;
use crate::uri::*;
use crate::message_parser::*;
use crate::body::Body;

// NOTE: Content in the HTTP headers is ISO-8859-1 so may contain characters outside the range of ASCII.
// type HttpStr = Vec<u8>;


// pub struct HttpResponseBuilder {
// 	status_code: HttpStatusCode,
// 	reason: String,
// 	headers: Vec<(String, String)>
// 	//
// }

// // Body must be either Read or a buffer
// // Regardless must be ov

// impl HttpResponseBuilder {
// 	pub fn new(status_code: HttpStatusCode, reason: String) -> Self {
// 		HttpResponseBuilder { status_code, reason, headers: vec![] }
// 	}

// 	pub fn header(&mut self, name: String, value: String) -> &mut Self {

// 	}

// 	pub fn build(&self) -> std::result::Result<> {

// 	}
// }

// use std::sync::Arc;
// trait Body : Arc {}

// TODO: Need validation of duplicate headers.


pub trait ToHeaderName {
	fn to_header_name(self) -> Result<AsciiString>;
}

impl<T: AsRef<[u8]>> ToHeaderName for T {
	fn to_header_name(self) -> Result<AsciiString> {
		let f = || {
			let s = AsciiString::from(self.as_ref())?;
			parse_field_name(s.data.clone())?;
			Ok(s)
		};
		
		f().map_err(|e: Error| format_err!("Invalid header name: {:?}", e))
	}
}

pub trait ToHeaderValue {
	fn to_header_value(self, name: &AsciiString) -> Result<Latin1String>;
}

impl<T: AsRef<str>> ToHeaderValue for T {
	fn to_header_value(self, name: &AsciiString) -> Result<Latin1String> {
		let f = || {
			let s = Latin1String::from(self.as_ref())?;
			parse_field_content(s.data.clone())?;
			Ok(s)
		};

		f().map_err(|e: Error| format_err!("Invalid value for header {}: {:?}", name.to_string(), e))
	}
}



const BODY_BUFFER_SIZE: usize = 4096;

pub struct Request {
	pub head: RequestHead,
	pub body: Box<dyn Body>
}

pub struct RequestBuilder {
	method: Option<Method>,
	uri: Option<Uri>,
	headers: Vec<HttpHeader>,
	body: Option<Box<dyn Body>>,

	// First error that occured in the building process
	error: Option<Error>
}

impl RequestBuilder {
	pub fn new() -> RequestBuilder {
		RequestBuilder {
			method: None, uri: None, headers: vec![], error: None, body: None }
	}

	pub fn method(mut self, method: Method) -> Self {
		self.method = Some(method);
		self
	}

	pub fn uri<U: AsRef<[u8]>>(mut self, uri: U) -> Self {
		// TODO: Implement a complete() parser combinator to deal with ensuring nothing is left
		self.uri = match complete(parse_request_target)(Bytes::from(uri.as_ref())) {
			Ok((u, _)) => Some(u.into_uri()),
			Err(e) => {
				self.error = Some(
					format_err!("Invalid request uri: {:?}", e));
				None
			}
		};

		self
	}

	// TODO: Currently this is the only safe way to build a request
	// we will need to dedup this with 
	pub fn header<N: ToHeaderName, V: ToHeaderValue>(
		mut self, name: N, value: V) -> Self {
		
		let name = match name.to_header_name() {
			Ok(v) => v,
			Err(e) => {
				self.error = Some(e);
				return self;
			}
		};

		let value = match value.to_header_value(&name) {
			Ok(v) => v,
			Err(e) => {
				self.error = Some(e);
				return self;
			}
		};

		self.headers.push(HttpHeader { name, value });
		self
	}

	pub fn body(mut self, body: Box<dyn Body>) -> Self {
		self.body = Some(body);
		self
	}

	pub fn build(self) -> Result<Request> {
		if let Some(e) = self.error {
			return Err(e);
		}

		let method = self.method
			.ok_or_else(|| err_msg("No method specified"))?;

		// TODO: Only certain types of uris are allowed here
		let uri = self.uri
			.ok_or_else(|| err_msg("No uri specified"))?;

		let headers = HttpHeaders::from(self.headers);

		let body = self.body
			.ok_or_else(|| err_msg("No body specified"))?;

		Ok(Request {
			head: RequestHead {
				method, uri,
				version: HTTP_V1_1,
				headers
			},
			body
		})
	}

}

pub struct ResponseBuilder {
	status_code: Option<StatusCode>,
	reason: Option<String>,
	headers: Vec<HttpHeader>,
	body: Option<Box<dyn Body>>,

	// First error that occured in the building process
	error: Option<Error>
}


impl ResponseBuilder {
	pub fn new() -> ResponseBuilder {
		ResponseBuilder { status_code: None, reason: None,
						  headers: vec![], body: None, error: None  }
	}

	pub fn status(mut self, code: StatusCode) -> Self {
		self.status_code = Some(code);
		self
	}

	pub fn header<N: ToHeaderName, V: ToHeaderValue>(
		mut self, name: N, value: V) -> Self {
		
		let name = match name.to_header_name() {
			Ok(v) => v,
			Err(e) => {
				self.error = Some(e);
				return self;
			}
		};

		let value = match value.to_header_value(&name) {
			Ok(v) => v,
			Err(e) => {
				self.error = Some(e);
				return self;
			}
		};

		self.headers.push(HttpHeader { name, value });
		self
	}

	pub fn body(mut self, body: Box<dyn Body>) -> Self {
		self.body = Some(body);
		self
	}

	pub fn build(self) -> Result<Response> {
		if let Some(e) = self.error {
			return Err(e);
		}

		let status_code = self.status_code
			.ok_or_else(|| err_msg("No status specified"))?;

		// TODO: Support custom reason and don't unwrap this.
		let reason = String::from(status_code.default_reason().unwrap());

		let headers = HttpHeaders::from(self.headers);

		let body = self.body
			.ok_or_else(|| err_msg("No body specified"))?;

		Ok(Response {
			head: ResponseHead {
				status_code, reason,
				version: HTTP_V1_1, // TODO: Always respond with version <= client version?
				headers
			},
			body
		})
	}
}



#[derive(Debug)]
pub struct RequestHead {
	// TODO: Only certain types of URIs are valid in this context
	pub method: Method,
	pub uri: Uri,
	pub version: HttpVersion,
	pub headers: HttpHeaders
}

impl RequestHead {
	pub fn serialize(&self, buf: &mut Vec<u8>) {
		let request_line = format!("{} {} HTTP/{}\r\n",
				std::str::from_utf8(self.method.as_str()).unwrap(),
				self.uri.to_string(), self.version.to_string());
		buf.extend_from_slice(request_line.as_bytes());

		self.headers.serialize(buf);
		buf.extend_from_slice(b"\r\n");
	}
}

// TODO: Instead just implement for head (or add some length info to describe the body)?
impl std::fmt::Debug for Request {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.head.fmt(f)
    }
}


pub struct Response {
	pub head: ResponseHead,
	pub body: Box<dyn Body>
}

#[derive(Debug)]
pub struct ResponseHead {
	pub version: HttpVersion,
	pub status_code: StatusCode,
	pub reason: String,
	pub headers: HttpHeaders,
}

impl ResponseHead {
	pub fn serialize(&self, out: &mut Vec<u8>) {
		let status_line = format!("HTTP/{} {} {}\r\n",
			self.version.to_string(),
			self.status_code.as_u16(),
			self.reason.to_string());
		out.extend_from_slice(status_line.as_bytes());

		self.headers.serialize(out);
		out.extend_from_slice(b"\r\n");
	}
}

use common::async_std::prelude::*;

// TODO: Move this out of the spec as it is the only async thing here.
// Probably move under Body
pub async fn write_body(mut body: &mut dyn Body, writer: &dyn Writeable)
	-> Result<()> {
	// TODO: If we sent a Content-Length, make sure that we are consistent.
	let mut buf = [0u8; BODY_BUFFER_SIZE];
	loop {
		let n = body.read(&mut buf).await?;
		if n == 0 {
			break;
		}

		writer.write_all(&buf[0..n]).await?;
	}

	Ok(())
}



#[derive(Debug, PartialEq)]
pub enum Method {
	GET,
	HEAD,
	POST,
	PUT,
	DELETE,
	CONNECT,
	OPTIONS,
	TRACE,
	PATCH
}

impl Method {
	pub fn as_str(&self) -> &'static [u8] {
		match self {
			Method::GET => b"GET",
			Method::HEAD => b"HEAD",
			Method::POST => b"POST",
			Method::PUT => b"PUT",
			Method::DELETE => b"DELETE",
			Method::CONNECT => b"CONNECT",
			Method::OPTIONS => b"OPTIONS",
			Method::TRACE => b"TRACE",
			Method::PATCH => b"PATCH"
		}
	}
}

impl std::convert::TryFrom<&[u8]> for Method {
	type Error = &'static str;
	fn try_from(value: &[u8]) -> std::result::Result<Self, Self::Error> {
		Ok(match value {
			b"GET" => Method::GET,
			b"HEAD" => Method::HEAD,
			b"POST" => Method::POST,
			b"PUT" => Method::PUT,
			b"DELETE" => Method::DELETE,
			b"CONNECT" => Method::CONNECT,
			b"OPTIONS" => Method::OPTIONS,
			b"TRACE" => Method::TRACE,
			b"PATCH" => Method::PATCH,
			_ => { return Err("Invalid method"); }
		})
	}
}


#[derive(Debug)]
pub enum StartLine {
	Request(RequestLine),
	Response(StatusLine)
}


#[derive(Debug)]
pub struct HttpHeader {
	pub name: AsciiString,
	pub value: Latin1String
}

impl HttpHeader {
	pub fn new(name: String, value: String) -> Self {
		Self {
			name: unsafe {
				AsciiString::from_ascii_unchecked(bytes::Bytes::from(name))
			},
			value: Latin1String::from_bytes(Bytes::from(value)).unwrap()
		}
	}

	pub fn serialize(&self, buf: &mut Vec<u8>) {
		buf.extend_from_slice(&self.name.data);
		buf.extend_from_slice(b": ");
		buf.extend_from_slice(&self.value.data);
		buf.extend_from_slice(b"\r\n");
	}
}


#[derive(Debug)]
pub struct HttpMessageHead {
	pub start_line: StartLine,	
	pub headers: HttpHeaders
}

#[derive(Debug)]
pub struct HttpHeaders {
	pub raw_headers: Vec<HttpHeader>
}

impl HttpHeaders {
	pub fn new() -> HttpHeaders {
		HttpHeaders { raw_headers: vec![] }
	}

	pub fn from(raw_headers: Vec<HttpHeader>) -> HttpHeaders {
		HttpHeaders { raw_headers }
	}

	/// Finds all headers matching a given name.
	pub fn find<'a>(&'a self, name: &'a [u8]) -> impl Iterator<Item=&'a HttpHeader> {
		self.raw_headers.iter().filter(move |h| {
			h.name.eq_ignore_case(name)
		})
	}

	pub fn has(&self, name: &[u8]) -> bool {
		for h in self.raw_headers.iter() {
			if h.name.eq_ignore_case(name) {
				return true;
			}
		}

		false
	}

	pub fn serialize(&self, buf: &mut Vec<u8>) {
		for h in &self.raw_headers {
			h.serialize(buf);
		}
	}
}

#[derive(Debug)]
pub struct RequestLine {
	pub method: AsciiString,
	pub target: RequestTarget,
	pub version: HttpVersion
}

#[derive(Debug, PartialEq, Eq)]
pub struct HttpVersion {
	pub major: u8,
	pub minor: u8
}

// TODO: Read https://www.ietf.org/rfc/rfc1945.txt, we should never actually see 0.9 in a one-liner

impl HttpVersion {
	pub fn to_string(&self) -> String {
		format!("{}.{}", self.major, self.minor)
	}
}

pub const HTTP_V0_9: HttpVersion = HttpVersion { major: 0, minor: 9 };
pub const HTTP_V1_0: HttpVersion = HttpVersion { major: 1, minor: 0 };
pub const HTTP_V1_1: HttpVersion = HttpVersion { major: 1, minor: 1 };
pub const HTTP_V2_0: HttpVersion = HttpVersion { major: 2, minor: 0 };


#[derive(Debug)]
pub struct Protocol {
	pub name: AsciiString,
	pub version: Option<AsciiString>
}




// https://tools.ietf.org/html/rfc7230#section-5.3
#[derive(Debug)]
pub enum RequestTarget {
	/// Standard relative path. This is the typical request
	OriginForm(Vec<AsciiString>, Option<AsciiString>),

	/// Typically a proxy request
	/// NOTE: Must be accepted ALWAYS be servers.
	AbsoluteForm(Uri),
	
	/// Only used for CONNECT.
	AuthorityForm(Authority),
	
	/// Used for OPTIONS.
	AsteriskForm
}

impl RequestTarget {
	pub fn into_uri(self) -> Uri {
		match self {
			RequestTarget::OriginForm(path_abs, query) => Uri {
				scheme: None,
				authority: None,
				path: UriPath::Absolute(path_abs).to_string(),
				query,
				fragment: None
			},
			RequestTarget::AbsoluteForm(u) => u,
			RequestTarget::AuthorityForm(a) => Uri {
				scheme: None, authority: Some(a),
				// TODO: Wrong?
				path: String::new(), query: None, fragment: None
			},
			RequestTarget::AsteriskForm => Uri {
				scheme: None, authority: None,
				path: String::from("*"),
				query: None, fragment: None
			}			
		}
	}
}

#[derive(Debug)]
pub struct StatusLine {
	pub version: HttpVersion,
	pub status_code: u16,
	pub reason: Latin1String
}


