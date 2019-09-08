
use super::ascii::*;
use bytes::Bytes;
use std::io::Write;
use super::status_code::*;
use parsing::iso::*;
use parsing::complete;
use crate::message_parser::*;
use super::uri::*;
use common::errors::*;

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


trait ToHeaderName {
	fn to_header_name(self) -> Result<AsciiString>;
}

impl<T: AsRef<[u8]>> ToHeaderName for T {
	fn to_header_name(self) -> Result<AsciiString> {
		let f = || {
			let s = AsciiString::from(self.as_ref())?;
			parse_field_name(s.data.clone())?;
			Ok(s)
		};
		
		f().map_err(|e: Error| format!("Invalid header name: {:?}", e).into())
	}
}

trait ToHeaderValue {
	fn to_header_value(self, name: &AsciiString) -> Result<ISO88591String>;
}

impl<T: AsRef<str>> ToHeaderValue for T {
	fn to_header_value(self, name: &AsciiString) -> Result<ISO88591String> {
		let f = || {
			let s = ISO88591String::from(self.as_ref())?;
			parse_field_content(s.data.clone())?;
			Ok(s)
		};

		f().map_err(|e: Error| format!("Invalid value for header {}: {:?}", name.to_string(), e).into())
	}
}



const BODY_BUFFER_SIZE: usize = 4096;

pub struct Request {
	pub head: RequestHead,
	pub body: Box<dyn std::io::Read>
}

pub struct RequestBuilder {
	method: Option<Method>,
	uri: Option<Uri>,
	headers: Vec<HttpHeader>,
	body: Option<Box<dyn std::io::Read>>,

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
					format!("Invalid request uri: {:?}", e).into());
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

	pub fn body(mut self, body: Box<dyn std::io::Read>) -> Self {
		self.body = Some(body);
		self
	}

	pub fn build(self) -> Result<Request> {
		if let Some(e) = self.error {
			return Err(e);
		}

		let method = self.method
			.ok_or_else(|| Error::from("No method specified"))?;

		// TODO: Only certain types of uris are allowed here
		let uri = self.uri
			.ok_or_else(|| Error::from("No uri specified"))?;

		let headers = HttpHeaders::from(self.headers);

		let body = self.body
			.ok_or_else(|| Error::from("No body specified"))?;

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
	body: Option<Box<dyn std::io::Read>>,

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

	pub fn body(mut self, body: Box<dyn std::io::Read>) -> Self {
		self.body = Some(body);
		self
	}

	pub fn build(self) -> Result<Response> {
		if let Some(e) = self.error {
			return Err(e);
		}

		let status_code = self.status_code
			.ok_or_else(|| Error::from("No status specified"))?;

		// TODO: Support custom reason and don't unwrap this.
		let reason = String::from(status_code.default_reason().unwrap());

		let headers = HttpHeaders::from(self.headers);

		let body = self.body
			.ok_or_else(|| Error::from("No body specified"))?;

		Ok(Response {
			head: ResponseHead {
				status_code, reason,
				version: HTTP_V1_1,
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
	pub head: ResponseHead,
	pub body: Box<dyn std::io::Read>
}

#[derive(Debug)]
pub struct ResponseHead {
	pub version: HttpVersion,
	pub status_code: StatusCode,
	pub reason: String,
	pub headers: HttpHeaders,
}

impl Response {
	pub fn serialize(&mut self, writer: &mut dyn Write) -> std::io::Result<()> {
		let status_line = format!("HTTP/{} {} {}\r\n",
			self.head.version.to_string(),
			self.head.status_code.as_u16(),
			self.head.reason.to_string());
		writer.write_all(status_line.as_bytes())?;

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
	pub fn serialize(&self, writer: &mut dyn Write) -> std::io::Result<()> {
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

	pub fn serialize(&self, writer: &mut dyn Write) -> std::io::Result<()> {
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


