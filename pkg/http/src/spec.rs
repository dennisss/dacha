
use super::ascii::*;
use bytes::Bytes;
use std::io::Write;
use super::status_code::*;

// NOTE: Content in the HTTP headers is ISO-8859-1 so may contain characters outside the range of ASCII.
type HttpStr = Vec<u8>;


// ISO-8859-1 string reference
// (https://en.wikipedia.org/wiki/ISO/IEC_8859-1)
pub struct ISO88591String {
	// All bytes must be in the ranges:
	// - [0x20, 0x7E]
	// - [0xA0, 0xFF] 
	pub data: Bytes
}

impl ISO88591String {
	pub fn from_bytes(data: Bytes) -> std::result::Result<ISO88591String, String> {
		for i in &data {
			let valid = (*i >= 0x20 && *i <= 0x7e) ||
						(*i >= 0xa0);
			if !valid {
				return Err(format!("Undefined ISO-8859-1 code point: {:x}", i));
			}
		}

		Ok(ISO88591String { data })
	}

	/// Converts to a standard utf-8 string.
	pub fn to_string(&self) -> String {
		let mut s = String::new();
		for i in &self.data {
			let c = std::char::from_u32(*i as u32).expect("Invalid character");
			s.push(c);
		}

		s
	}
}

impl std::fmt::Debug for ISO88591String {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		self.to_string().fmt(f)
    }
}


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

const BODY_BUFFER_SIZE: usize = 4096;

pub struct Request {
	pub head: RequestHead,
	pub body: Box<std::io::Read>
}

#[derive(Debug)]
pub struct RequestHead {
	// TODO: Only certain types of URIs are valid in this context
	pub method: Method,
	pub uri: Uri,
	pub version: HttpVersion,
	pub headers: HttpHeaders
}

impl Request {
	pub fn serialize(&mut self, writer: &mut Write) -> std::io::Result<()> {
		let request_line = format!("{} {} HTTP/{}\r\n",
				std::str::from_utf8(self.head.method.as_str()).unwrap(), self.head.uri.to_string(), self.head.version.to_string());
		writer.write_all(request_line.as_bytes());

		/////

		self.head.headers.serialize(writer)?;
		writer.write_all(b"\r\n")?;
		
		// TODO: If we sent a Content-Length, make sure that we are consistent.
		let mut buf = [0u8; BODY_BUFFER_SIZE];
		loop {
			let n = self.body.read(&mut buf)?;
			if n == 0 {
				break;
			}

			writer.write_all(&buf[0..n])?;
		}

		Ok(())
	}
}

impl std::fmt::Debug for Request {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.head.fmt(f)
    }
}


pub struct Response {
	pub version: HttpVersion,
	pub status_code: StatusCode,
	pub reason: String,
	pub headers: HttpHeaders,
	pub body: Box<std::io::Read>
}

impl Response {
	pub fn serialize(&mut self, writer: &mut Write) -> std::io::Result<()> {
		let status_line = format!("HTTP/{} {} {}\r\n", self.version.to_string(),
			self.status_code.as_u16(), self.reason.to_string());
		writer.write_all(status_line.as_bytes())?;

		/////

		self.headers.serialize(writer)?;
		writer.write_all(b"\r\n")?;
		
		// TODO: If we sent a Content-Length, make sure that we are consistent.
		let mut buf = [0u8; BODY_BUFFER_SIZE];
		loop {
			let n = self.body.read(&mut buf)?;
			if n == 0 {
				break;
			}

			writer.write_all(&buf[0..n])?;
		}

		Ok(())
	}
}


#[derive(Debug)]
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
	fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
		Ok(match value {
			b"GET" => Method::GET,
			b"HEAD" => Method::HEAD,
			b"POST" => Method::POST,
			b"PUT" => Method::PUT,
			b"DELETE" => Method::DELETE,
			b"CONNECT" => Method::CONNECT,
			b"OPTIONS" => Method::OPTIONS,
			b"TRACE" => Method::TRACE,
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
	pub value: ISO88591String
}

impl HttpHeader {
	pub fn serialize(&self, writer: &mut Write) -> std::io::Result<()> {
		writer.write_all(&self.name.data)?;
		writer.write_all(b": ")?;
		writer.write_all(&self.value.data)?;
		writer.write_all(b"\r\n");
		Ok(())
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
	pub fn from(raw_headers: Vec<HttpHeader>) -> HttpHeaders {
		HttpHeaders { raw_headers }
	}

	/// Finds all headers matching a given name.
	pub fn find<'a>(&'a self, name: &'a [u8]) -> impl Iterator<Item=&'a HttpHeader> {
		self.raw_headers.iter().filter(move |h| {
			h.name.eq_ignore_case(name)
		})
	}

	pub fn serialize(&self, writer: &mut Write) -> std::io::Result<()> {
		for h in &self.raw_headers {
			h.serialize(writer)?;
		}
		Ok(())
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
	// Standard relative path. This is the typical request
	OriginForm(Vec<AsciiString>, Option<AsciiString>),
	// Typically a proxy request
	// NOTE: Must be accepted ALWAYS be servers.
	AbsoluteForm(Uri),
	
	// Only used for CONNECT.
	AuthorityForm(Authority),
	
	// Used for OPTIONS.
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
	pub reason: ISO88591String
}


#[derive(Debug)]
pub struct Uri {
	pub scheme: Option<AsciiString>,
	pub authority: Option<Authority>,
	pub path: String,
	pub query: Option<AsciiString>,
	// NOTE: This will always be empty for absolute_uri
	pub fragment: Option<AsciiString>
}

impl Uri {
	// TODO: Encode any characters that we need to encode for this.
	pub fn to_string(&self) -> String {
		let mut out = String::new();
		if let Some(scheme) = &self.scheme {
			out += &format!("{}:", scheme.to_string());
		}

		// TODO: Authority

		out += &self.path;

		if let Some(query) = &self.query {
			out += &format!("?{}", query.to_string());
		}

		if let Some(fragment) = &self.fragment {
			out += &format!("#{}", fragment.to_string());
		}

		out
	}
}


#[derive(Debug)]
pub struct Authority {
	pub user: Option<AsciiString>,
	pub host: Host,
	pub port: Option<usize>
}

#[derive(Debug)]
pub enum Host {
	Name(AsciiString), // NOTE: This is strictly ASCII.
	IP(IPAddress)
}

#[derive(Debug)]
pub enum IPAddress {
	V4(Vec<u8>),
	V6(Vec<u8>),
	VFuture(Vec<u8>)
}

#[derive(Debug)]
pub enum UriPath {
	AbEmpty(Vec<AsciiString>),
	Absolute(Vec<AsciiString>),
	Rootless(Vec<AsciiString>),
	Empty
}

impl UriPath {
	pub fn to_string(&self) -> String {
		let join = |strs: &Vec<AsciiString>| {
			strs.iter().map(|s| s.to_string()).collect::<Vec<_>>().join("/")
		};

		match self {
			UriPath::AbEmpty(v) => format!("/{}", join(v)),
			UriPath::Absolute(v) => format!("/{}", join(v)),
			UriPath::Rootless(v) => join(v),
			UriPath::Empty => String::new(),
		}
	}
}


#[derive(Debug)]
pub enum Path {
	PathAbEmpty(Vec<AsciiString>),
	PathAbsolute(Vec<AsciiString>),
	PathNoScheme(Vec<AsciiString>),
	PathRootless(Vec<AsciiString>),
	PathEmpty
}
