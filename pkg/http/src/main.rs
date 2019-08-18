#![feature(core_intrinsics)]

#[macro_use] extern crate parsing;
extern crate bytes;

use common::errors::*;
use parsing::*;

mod status_code;
mod spec;
mod ascii;

use std::io;
use std::io::{Read, Write, Cursor};
use std::net::{TcpListener, TcpStream};
use bytes::Bytes;
use std::sync::{Arc, Mutex};
use spec::*;
use ascii::*;
use status_code::*;
use std::thread;

// Marker that indicates the end of the HTTP headers.
const http_endmarker: &'static [u8] = b"\r\n\r\n";

const buffer_size: usize = 1024;

// If we average an http preamable (request line + headers) larger than this size, then we will fail the request.
const max_buffer_size: usize = 16*1024; // 16KB

fn EmptyBody() -> Box<Read> {
	Box::new(Cursor::new(Vec::new()))
}

fn BodyFromData(data: Vec<u8>) -> Box<Read> {
	Box::new(Cursor::new(data))
}


struct OutgoingBody {
	stream: Arc<Mutex<TcpStream>>
}

impl Write for OutgoingBody {
	fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
		let mut guard = self.stream.lock().unwrap();
		let s: &mut TcpStream = guard.borrow_mut();
		s.write(buf)
	}
	fn flush(&mut self) -> std::io::Result<()> {
		let mut guard = self.stream.lock().unwrap();
		let s: &mut TcpStream = guard.borrow_mut();
		s.flush()
	}
}

use std::sync::mpsc;

enum Chunk {
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






// `Content-Length = 1*DIGIT`
fn parse_content_length(headers: &HttpHeaders) -> Result<Option<usize>> {
	let mut hs = headers.find(b"Content-Length");
	let len = if let Some(h) = hs.next() {
		if let Ok(v) = usize::from_str_radix(&h.value.to_string(), 10) {
			Some(v)
		} else {
			return Err(format!("Invalid Content-Length: {:?}", h.value).into());
		}
	} else {
		// No header present.
		None
	};

	// Having more than one header is an error.
	if !hs.next().is_none() {
		return Err("More than one Content-Length header received.".into());
	}

	Ok(len)
}


/// 
struct IncomingBody {
	// Current position in body (incremented on reads).
	idx: usize,
	// Number of bytes we expect (if a Content-Length header was given).
	length: Option<usize>,
	// Extra bytes already read after the end of the 
	// TODO: This may contain extra bytes after completion for the next request.
	head: Bytes,

	stream: Arc<Mutex<TcpStream>>
}

impl Read for IncomingBody {
	fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
		// TODO: Should block reading after the response has been sent (aka error out). 

		if Some(self.idx) == self.length {
			return Ok(0);
		}

		let mut rest = buf;
		let mut total_read = 0;

		if rest.len() > 0 && self.idx < self.head.len() {
			let n = std::cmp::min(rest.len(), self.head.len() - self.idx);
			rest[0..n].copy_from_slice(&self.head[self.idx..(self.idx + n)]);
			total_read += n;
			rest = &mut rest[n..];
			self.idx += n;
		}

		if rest.len() > 0 && self.idx < self.head.len() {
			let n = if let Some(length) = self.length {
				std::cmp::min(rest.len(), length - self.idx) 
			} else {
				rest.len()
			};

			if n > 0 {
				let mut s = self.stream.lock().unwrap();
				let nread = s.read(&mut rest[0..n])?;
				self.idx += nread;
				total_read += nread;
			}
		}

		Ok(total_read)
	}
}

// TODO: Pipelining?

// For a TCP connection, this will 
// struct Body {

// }

// fn BodyFromBytes(data: Vec<>)

use std::convert::TryFrom;
use std::borrow::BorrowMut;

fn handle_client(mut stream: TcpStream) -> std::io::Result<()> {

	// Remaining bytes from the last 
	let mut last_remaining = None;

	let mut buf = vec![];
	buf.resize(buffer_size, 0u8);

	// Index up to which we have read.
	let mut idx = 0;

	// Index of the start of the http body (end of the headers).
	let mut body_idx = 0;

	// Read until we see the end marker and overflow our buffer limit.
	loop {
		// TODO: When we get the first CRLF, check if we got a 0.9 request

		let nread = stream.read(&mut buf[idx..])?;
		idx += nread;

		let mut found = false;
		for i in 0..(idx - http_endmarker.len() + 1) {
			let j = i + http_endmarker.len();
			if &buf[i..j] == http_endmarker {
				body_idx = j;
				found = true;
				break;
			}
		}

		if found {
			break;
		}

		if buf.len() - idx < buffer_size {
			let num_to_add = std::cmp::min(
				buffer_size,
				max_buffer_size - buf.len()
			);

			if num_to_add == 0 {
				stream.write_all(b"HTTP/1.1 431 Request Header Fields Too Large\r\n\r\n")?;
				return Ok(());
			}

			let new_size = buf.len() + num_to_add;
			buf.resize(new_size, 0u8);
		}
	}

	let b = Bytes::from(buf);

	let head = b.slice(0..body_idx);
	let msg = match parse_http_message_head(head) {
		Ok((msg, rest)) => {
			assert_eq!(rest.len(), 0);	
			msg
		},
		Err(e) => {
			println!("Failed to parse message\n{}", e);
			stream.write_all(b"HTTP/1.1 400 Bad Request\r\n\r\n")?;
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
			stream.write_all(b"HTTP/1.1 400 Bad Request\r\n\r\n")?;
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
			stream.write_all(b"HTTP/1.1 505 HTTP Version Not Supported\r\n\r\n")?;
			return Ok(())
		}
	};

	// Validate method
	let method = match Method::try_from(request_line.method.data.as_ref()) {
		Ok(m) => m,
		Err(_) => {
			println!("Unsupported http method: {:?}", request_line.method);
			stream.write_all(b"HTTP/1.1 405 Method Not Allowed\r\n\r\n")?;
			return Ok(());
		}
	};

	// TODO: Extract content-length and transfer-encoding

	let content_length = match parse_content_length(&headers) {
		Ok(len) => len,
		Err(e) => {
			println!("{}", e);
			stream.write_all(b"HTTP/1.1 400 Bad Request\r\n\r\n")?;
			return Ok(());
		}
	};

	println!("Content-Length: {:?}", content_length);

	let shared_stream = Arc::new(Mutex::new(stream));

	let mut remaining = b.slice(body_idx..);
	if let Some(len) = content_length {
		if len < remaining.len() {
			last_remaining = Some(remaining.split_off(len));
		}
	}

	let req = Request {
		head: RequestHead {
			method,
			uri: request_line.target.into_uri(),
			version: request_line.version,
			headers,
		},
		body: Box::new(IncomingBody {
			idx: 0,
			length: content_length,
			head: remaining,
			stream: shared_stream.clone()
		})
	};

	let mut res = handle_request(req);

	// TODO: Must always send 'Date' header.
	// TODO: Add 'Server' header


	let mut res_writer = OutgoingBody { stream: shared_stream.clone() };
	res.serialize(&mut res_writer);

	drop(res);
	drop(res_writer);

	// Next step is to 

	// In order to support echo, we can't lock the TcpStream in the serializer
	// stream.write_all(b"HTTP/1.1 200 OK\r\n\r\n")?;

	Ok(())
}

pub fn new_header(name: String, value: String) -> HttpHeader {
	HttpHeader {
		name: unsafe { AsciiString::from_ascii_unchecked(Bytes::from(name)) },
		value: ISO88591String::from_bytes(Bytes::from(value)).unwrap()
	}
}

// If we send back using a chunked encoding, 

fn handle_request(mut req: Request) -> Response {

	println!("GOT: {:?}", req);

	let mut data = vec![];
	req.body.read_to_end(&mut data).expect("Read failed");

	// println!("READ: {:?}", data);

	let res_headers = vec![
		new_header("Content-Length".to_string(), format!("{}", data.len()))
	];

	Response {
		status_code: OK,
		version: HTTP_V1_1, // TODO: Always respond with version <= client version?
		reason: OK.default_reason().unwrap_or("").to_owned(),
		headers: HttpHeaders::from(res_headers),
		body: BodyFromData(data)
	}
}

fn main() -> io::Result<()> {
	let listener = TcpListener::bind("127.0.0.1:8000")?;

	for stream in listener.incoming() {
		let s = stream?;
		thread::spawn(move || {
			match handle_client(s) {
				Ok(v) => {},
				Err(e) => println!("Client thread failed: {}", e)
			}
		});
	}
	Ok(())
}


// Syntax RFC: https://tools.ietf.org/html/rfc7230
// ^ Key thing being that 8bits per character in ISO-... encoding.

// 1#element => element *( OWS "," OWS element )



//////////////////

// TODO: "In the interest of robustness, servers SHOULD ignore any empty line(s) received where a Request-Line is expected. In other words, if the server is reading the protocol stream at the beginning of a message and receives a CRLF first, it should ignore the CRLF." - https://www.w3.org/Protocols/rfc2616/rfc2616-sec4.html


// TODO: See https://tools.ietf.org/html/rfc7230#section-6.7 for upgrade

// `BWS = OWS`
parser!(parse_bws<Bytes> => parse_ows);

//    Connection = *( "," OWS ) connection-option *( OWS "," [ OWS
//     connection-option ] )


// Parser for the entire HTTP 0.9 request.
// `Simple-Request = "GET" SP Request-URI CRLF`
// TODO: Check https://www.ietf.org/rfc/rfc1945.txt for exactly what is allowed in the Uri are allowed
parser!(parse_simple_request<Uri> => {
	seq!(c => {
		c.next(tag("GET"))?;
		c.next(like(is_sp))?;
		let uri = c.next(parse_request_target)?.into_uri();
		c.next(parse_crlf)?;
		Ok(uri)
	})
});


// NOTE: This does not parse the body
// `HTTP-message = start-line *( header-field CRLF ) CRLF [ message-body ]`
parser!(parse_http_message_head<HttpMessageHead> => {
	seq!(c => {
		let start_line = c.next(parse_start_line)?;
		let raw_headers = c.next(many(seq!(c => {
			let h = c.next(parse_header_field)?;
			c.next(parse_crlf)?;
			Ok(h)
		})))?;

		c.next(parse_crlf)?;
		Ok(HttpMessageHead {
			start_line, headers: HttpHeaders::from(raw_headers)
		})
	})
});

// `HTTP-name = %x48.54.54.50 ; HTTP`
parser!(parse_http_name<()> => {
	map(tag("HTTP"), |_| ())
});

// `HTTP-version = HTTP-name "/" DIGIT "." DIGIT`
parser!(parse_http_version<HttpVersion> => {
	let digit = |input| {
		let (i, rest) = any(input)?;
		let v: [u8; 1] = [i];
		let s = std::str::from_utf8(&v).map_err(|e| e.to_string())?;
		let d = u8::from_str_radix(s, 10).map_err(|e| e.to_string())?;
		Ok((d, rest))
	};

	seq!(c => {
		c.next(parse_http_name)?;
		c.next(one_of("/"))?;
		let major = c.next(digit)?;
		c.next(one_of("."))?;
		let minor = c.next(digit)?;
		Ok(HttpVersion {
			major, minor
		})
	})
});


// TODO: Well known uri: https://tools.ietf.org/html/rfc8615


// `Host = uri-host [ ":" port ]`

// Optional whitespace
// `OWS = *( SP / HTAB )`
parser!(parse_ows<Bytes> => {
	take_while(|i| is_sp(i) || is_htab(i))
});

// Required whitespace
// `RWS = 1*( SP / HTAB )`
parser!(parse_rws<Bytes> => {
	take_while1(|i| is_sp(i) || is_htab(i))
});

//    TE = [ ( "," / t-codings ) *( OWS "," [ OWS t-codings ] ) ]
//    Trailer = *( "," OWS ) field-name *( OWS "," [ OWS field-name ] )
//    Transfer-Encoding = *( "," OWS ) transfer-coding *( OWS "," [ OWS
//     transfer-coding ] )

//    URI-reference = <URI-reference, see [RFC3986], Section 4.1>
//    Upgrade = *( "," OWS ) protocol *( OWS "," [ OWS protocol ] )

//    Via = *( "," OWS ) ( received-protocol RWS received-by [ RWS comment
//     ] ) *( OWS "," [ OWS ( received-protocol RWS received-by [ RWS
//     comment ] ) ] )

// `absolute-form = absolute-URI`
parser!(parse_absolute_form<Uri> => parse_absolute_uri);

// NOTE: This is strictly ASCII.
// `absolute-path = 1*( "/" segment )`
parser!(parse_absolute_path<Vec<AsciiString>> => {
	many1(seq!(c => {
		c.next(one_of("/"))?;
		c.next(parse_segment)
	}))
});

// `asterisk-form = "*"`
parser!(parse_asterisk_form<u8> => one_of("*"));

// `authority-form = authority`
parser!(parse_authority_form<Authority> => parse_authority);

// `chunk = chunk-size [ chunk-ext ] CRLF chunk-data CRLF`
//    chunk-data = 1*OCTET
//    chunk-ext = *( ";" chunk-ext-name [ "=" chunk-ext-val ] )
//    chunk-ext-name = token
//    chunk-ext-val = token / quoted-string
//    chunk-size = 1*HEXDIG
//    chunked-body = *chunk last-chunk trailer-part CRLF
//    comment = "(" *( ctext / quoted-pair / comment ) ")"

// `connection-option = token`
parser!(parse_connection_option<AsciiString> => {
	parse_token
});


// `ctext = HTAB / SP / %x21-27 ; '!'-'''
//     	  / %x2A-5B ; '*'-'['
//     	  / %x5D-7E ; ']'-'~'
//     	  / obs-text`

// TODO: See also https://tools.ietf.org/html/rfc8187
// It's not entirely clear if the header can be non-ASCII, but for now, we leave it to be ISO-
// `field-content = field-vchar [ 1*( SP / HTAB ) field-vchar ]`
parser!(parse_field_content<Bytes> => {
	slice(seq!(c => {
		c.next(like(is_field_vchar))?;

		c.next(opt(seq!(c => {
			c.next(take_while1(|i| is_sp(i) || is_htab(i)))?;
			c.next(like(is_field_vchar))
		})))
	}))
});

// NOTE: This is strictly ASCII.
// `field-name = token`
parser!(parse_field_name<AsciiString> => parse_token);

// TODO: Perform special error to client if we get obs-fold
// `field-value = *( field-content / obs-fold )`
parser!(parse_field_value<ISO88591String> => {
	and_then(slice(many(alt!(
		parse_field_content, parse_obs_fold
	))), |v| ISO88591String::from_bytes(v))
});

// `field-vchar = VCHAR / obs-text`
fn is_field_vchar(i: u8) -> bool { is_vchar(i) || is_obs_text(i) }

// `header-field = field-name ":" OWS field-value OWS`
parser!(parse_header_field<HttpHeader> => {
	seq!(c => {
		let name = c.next(parse_field_name)?;
		c.next(one_of(":"))?;
		c.next(parse_ows)?;
		let value = c.next(parse_field_value)?;
		c.next(parse_ows)?;
		Ok(HttpHeader { name, value })
	})
});

// `http-URI = "http://" authority path-abempty [ "?" query ] [ "#"
//     fragment ]`

// `https-URI = "https://" authority path-abempty [ "?" query ] [ "#"
//     fragment ]`

// `last-chunk = 1*"0" [ chunk-ext ] CRLF`

// `message-body = *OCTET`

// `method = token`
parser!(parse_method<AsciiString> => parse_token);

// `obs-fold = CRLF 1*( SP / HTAB )`
parser!(parse_obs_fold<Bytes> => {
	slice(seq!(c => {
		c.next(tag("\r\n"))?;
		c.next(take_while1(|i| is_sp(i) || is_htab(i)))?;
		println!("FOLD");
		Ok(())
	}))
});

// TODO: 128 to 159 are undefined in ISO-8859-1
// (Obsolete) Text
// `obs-text = %x80-FF`
fn is_obs_text(i: u8) -> bool { i >= 0x80 && i <= 0xff }

// `origin-form = absolute-path [ "?" query ]`
parser!(parse_origin_form<(Vec<AsciiString>, Option<AsciiString>)> => {
	seq!(c => {
		let abspath = c.next(parse_absolute_path)?;
		let q = c.next(opt(seq!(c => {
			c.next(one_of("?"))?;
			c.next(parse_query)
		})))?;

		Ok((abspath, q))
	})
});

// `partial-URI = relative-part [ "?" query ]`


// `protocol = protocol-name [ "/" protocol-version ]`
parser!(parse_protocol<Protocol> => {
	seq!(c => {
		let name = c.next(parse_protocol_name)?;
		let version = c.next(opt(seq!(c => {
			c.next(tag("/"))?;
			c.next(parse_protocol_version)
		})))?;

		Ok(Protocol {
			name, version
		})
	})
});


// `protocol-name = token`
parser!(parse_protocol_name<AsciiString> => parse_token);

// `protocol-version = token`
parser!(parse_protocol_version<AsciiString> => parse_token);

// `pseudonym = token`
parser!(parse_pseudonym<AsciiString> => parse_token);


// `qdtext = HTAB / SP / "!" / %x23-5B ; '#'-'['
//         / %x5D-7E ; ']'-'~'
//         / obs-text`

// `quoted-pair = "\" ( HTAB / SP / VCHAR / obs-text )`

// `quoted-string = DQUOTE *( qdtext / quoted-pair ) DQUOTE`

// `rank = ( "0" [ "." *3DIGIT ] ) / ( "1" [ "." *3"0" ] )`

// TODO: Because of obs-text, this is not necessarily ascii
// `reason-phrase = *( HTAB / SP / VCHAR / obs-text )`
parser!(parse_reason_phrase<ISO88591String> => {
	and_then(take_while(|i| is_htab(i) || is_sp(i) ||
			 	   		    is_vchar(i) || is_obs_text(i)),
		|v| ISO88591String::from_bytes(v)) 
});

// `received-by = ( uri-host [ ":" port ] ) / pseudonym`

// `received-protocol = [ protocol-name "/" ] protocol-version`

// `request-line = method SP request-target SP HTTP-version CRLF`
parser!(parse_request_line<RequestLine> => {
	seq!(c => {
		let m = c.next(parse_method)?;
		c.next(sp)?;
		let t = c.next(parse_request_target)?;
		c.next(sp)?;
		let v = c.next(parse_http_version)?;
		c.next(parse_crlf)?;
		Ok(RequestLine { method: m, target: t, version: v })
	})
});

// `request-target = origin-form / absolute-form / authority-form / asterisk-form`
parser!(parse_request_target<RequestTarget> => {
	alt!(
		map(parse_origin_form, |(p, q)| RequestTarget::OriginForm(p, q)),
		map(parse_absolute_form, |u| RequestTarget::AbsoluteForm(u)),
		map(parse_authority_form, |a| RequestTarget::AuthorityForm(a)),
		map(parse_asterisk_form, |_| RequestTarget::AsteriskForm)
	)
});

// `start-line = request-line / status-line`
parser!(parse_start_line<StartLine> => {
	alt!(
		map(parse_request_line, |l| StartLine::Request(l)),
		map(parse_status_line, |l| StartLine::Response(l))
	)
});


// `status-code = 3DIGIT`
fn parse_status_code(input: Bytes) -> ParseResult<u16> {
	if input.len() < 3 { return Err("status_code: input too short".into()) }
	let s = std::str::from_utf8(&input[0..3]).map_err(|e| e.to_string())?;
	let code = u16::from_str_radix(s, 10).map_err(|e| e.to_string())?;
	Ok((code, input.slice(3..)))
}

// `status-line = HTTP-version SP status-code SP reason-phrase CRLF`
parser!(parse_status_line<StatusLine> => {
	seq!(c => {
		let version = c.next(parse_http_version)?;
		c.next(sp)?;
		let s = c.next(parse_status_code)?;
		c.next(sp)?;
		let reason = c.next(parse_reason_phrase)?;
		c.next(parse_crlf)?;
		Ok(StatusLine { version, status_code: s, reason })
	})
});

//    t-codings = "trailers" / ( transfer-coding [ t-ranking ] )
//    t-ranking = OWS ";" OWS "q=" rank

// NOTE: This is strictly ASCII.
// `tchar = "!" / "#" / "$" / "%" / "&" / "'" / "*" / "+" / "-" / "." /
//  "^" / "_" / "`" / "|" / "~" / DIGIT / ALPHA`
fn is_tchar(i: u8) -> bool {
	(i as char).is_alphanumeric() || is_one_of("!#$%&'*+-.^_`|~", i)
}

// NOTE: This is strictly ASCII.
// `token = 1*tchar`
fn parse_token(input: Bytes) -> ParseResult<AsciiString> {
	let (v, rest) = take_while1(is_tchar)(input)?;

	// This works because tchar will only ever access ASCII characters which
	// are a subset of UTF-8
	let s = unsafe { AsciiString::from_ascii_unchecked(v) };
	Ok((s, rest))
}

// trailer-part = *( header-field CRLF )
// transfer-coding = "chunked" / "compress" / "deflate" / "gzip" /
//  transfer-extension
// transfer-extension = token *( OWS ";" OWS transfer-parameter )
// transfer-parameter = token BWS "=" BWS ( token / quoted-string )

//    uri-host = <host, see [RFC3986], Section 3.2.2>

parser!(parse_crlf<Bytes> => tag("\r\n"));

fn is_sp(i: u8) -> bool { i == (' ' as u8) }
fn sp(input: Bytes) -> ParseResult<u8> { like(is_sp)(input) }

fn is_htab(i: u8) -> bool { i == ('\t' as u8) }

// Visible USASCII character.
fn is_vchar(i: u8) -> bool {
	i >= 0x21 && i <= 0x7e
}

//////////////////////////

// TODO: Ensure URLs never get 2K bytes (especially in the incremental form)
// https://stackoverflow.com/questions/417142/what-is-the-maximum-length-of-a-url-in-different-browsers


// `URI = scheme ":" hier-part [ "?" query ] [ "#" fragment ]`
fn parse_uri(input: Bytes) -> ParseResult<Uri> {
	let p = seq!(c => {
		let mut u = c.next(parse_absolute_uri)?;
		u.fragment = c.next(opt(seq!(c => {
			c.next(one_of("#"))?;
			c.next(parse_fragment)
		})))?;

		Ok(u)
	});

	p(input)
}

// `hier-part = "//" authority path-abempty
// 			  / path-absolute
// 			  / path-rootless
// 			  / path-empty`
parser!(parse_hier_part<(Option<Authority>, UriPath)> => {
	alt!(
		seq!(c => {
			c.next(tag("//"))?;
			let a = c.next(parse_authority)?;
			let p = c.next(parse_path_abempty)?;
			Ok((Some(a), UriPath::AbEmpty(p)))
		}),
		map(parse_path_absolute, |p| (None, UriPath::Absolute(p))),
		map(parse_path_rootless, |p| (None, UriPath::Rootless(p))),
		map(parse_path_empty, |p| (None, UriPath::Empty))
	)
});

// `URI-reference = URI / relative-ref`

// `absolute-URI = scheme ":" hier-part [ "?" query ]`
parser!(parse_absolute_uri<Uri> => {
	seq!(c => {
		let s = c.next(parse_scheme)?;
		c.next(one_of(":"))?;
		let (auth, p) = c.next(parse_hier_part)?;
		let q = c.next(opt(seq!(c => {
			c.next(one_of("?"))?;
			c.next(parse_query)
		})))?;

		Ok(Uri { scheme: Some(s), authority: auth, path: p.to_string(), query: q, fragment: None })
	})
});

// `relative-ref = relative-part [ "?" query ] [ "#" fragment ]`

// `relative-part = "//" authority path-abempty
// 				  / path-absolute
// 				  / path-noscheme
// 				  / path-empty`

// NOTE: This is strictly ASCII.
// `scheme = ALPHA *( ALPHA / DIGIT / "+" / "-" / "." )`
fn parse_scheme(input: Bytes) -> ParseResult<AsciiString> {
	let mut i = 0;
	while i < input.len() {
		let c = input[i];
		let valid = if i == 0 {
			(c as char).is_ascii_alphabetic()
		} else {
			(c as char).is_alphanumeric() || is_one_of("+-.", c)
		};

		if !valid {
			break;
		}

		i += 1;
	}

	if i < 1 {
		Err("scheme failed".into())
	} else {
		let mut v = input.clone();
		let rest = v.split_off(i);
		let s = unsafe { AsciiString::from_ascii_unchecked(v) };
		Ok((s, rest))
	}
}

// `authority = [ userinfo "@" ] host [ ":" port ]`
parser!(parse_authority<Authority> => {
	seq!(c => {
		let user = c.next(opt(seq!(c => {
			let u = c.next(parse_userinfo)?;
			c.next(one_of("@"))?;
			Ok(u)
		})))?;

		let h = c.next(parse_host)?;

		let p = c.next(seq!(c => {
			c.next(one_of(":"))?;
			c.next(parse_port)
		}))?;

		Ok(Authority { user, host: h, port: p })
	})
});


// `userinfo = *( unreserved / pct-encoded / sub-delims / ":" )`
parser!(parse_userinfo<AsciiString> => {
	map(many(alt!(
		parse_unreserved, parse_pct_encoded, parse_sub_delims, one_of(":")
	)), |s| unsafe { AsciiString::from_ascii_unchecked(Bytes::from(s)) })
});

// `host = IP-literal / IPv4address / reg-name`
parser!(parse_host<Host> => {
	alt!(
		map(parse_ip_literal, |i| Host::IP(i)),
		map(parse_ipv4_address, |v| Host::IP(IPAddress::V4(v))),
		map(parse_reg_name, |v| Host::Name(v))
	)
});

// `port = *DIGIT`
fn parse_port(input: Bytes) -> ParseResult<Option<usize>> {
	let (v, rest) = take_while1(|i| (i as char).is_digit(10))(input)?;
	if v.len() == 0 {
		return Ok((None, rest));
	}
	let s = std::str::from_utf8(&v).map_err(|e| e.to_string())?;
	let p = usize::from_str_radix(s, 10).map_err(|e| e.to_string())?;
	Ok((Some(p), rest))
}

// TODO: See also https://tools.ietf.org/html/rfc2047

// TODO: Add IPv6addrz as in https://tools.ietf.org/html/rfc6874
// `IP-literal = "[" ( IPv6address / IPvFuture  ) "]"`
parser!(parse_ip_literal<IPAddress> => {
	seq!(c => {
		c.next(one_of("["))?;
		let addr = c.next(alt!(
			map(parse_ipv6_address, |v| IPAddress::V6(v)),
			map(parse_ip_vfuture, |v| IPAddress::VFuture(v))
		))?;
		c.next(one_of("]"))?;
		Ok(addr)
	})
});

// `IPvFuture = "v" 1*HEXDIG "." 1*( unreserved / sub-delims / ":" )`
parser!(parse_ip_vfuture<Vec<u8>> => {
	seq!(c => {
		let mut out = vec![];
		out.push(c.next(one_of("v"))?);
		out.push(c.next(like(|i| (i as char).is_digit(16)))?);
		out.push(c.next(one_of("."))?);
		
		let rest = c.next(many1(alt!(
			parse_unreserved, parse_sub_delims, one_of(":")
		)))?;
		out.extend_from_slice(&rest);
		Ok(out)
	})
});


// `IPv6address =                            6( h16 ":" ) ls32
// 				/                       "::" 5( h16 ":" ) ls32
// 				/ [               h16 ] "::" 4( h16 ":" ) ls32
// 				/ [ *1( h16 ":" ) h16 ] "::" 3( h16 ":" ) ls32
// 				/ [ *2( h16 ":" ) h16 ] "::" 2( h16 ":" ) ls32
// 				/ [ *3( h16 ":" ) h16 ] "::"    h16 ":"   ls32
// 				/ [ *4( h16 ":" ) h16 ] "::"              ls32
// 				/ [ *5( h16 ":" ) h16 ] "::"              h16
// 				/ [ *6( h16 ":" ) h16 ] "::"`
fn parse_ipv6_address(input: Bytes) -> ParseResult<Vec<u8>> {
	let many_h16 = |n: usize| {
		seq!(c => {
			let mut out = vec![];
			for i in 0..n {
				out.extend(c.next(parse_h16)?.into_iter());
				c.next(one_of(":"))?;
			}
			Ok(out)
		})
	};
	
	let p = alt!(
		seq!(c => {
			let mut out = c.next(many_h16(6))?;
			out.extend(c.next(parse_ls32)?.into_iter());
			Ok(out)
		}),
		seq!(c => {
			c.next(tag("::"))?;
			let mut out = c.next(many_h16(6))?;
			out.extend(c.next(parse_ls32)?.into_iter());
			Ok(out)
		})
		// TODO: Need to implement all cases and fill in missing bytes

		// seq!(c => {
		// 	let out = c.next(opt(h16))
		// })
	);

	p(input)
}

// `h16 = 1*4HEXDIG`
fn parse_h16(input: Bytes) -> ParseResult<Vec<u8>> {
	if input.len() < 4 { return Err("h16: input too short".into()); }
	for i in 0..4 {
		if !(input[i] as char).is_digit(16) {
			return Err("h16 not digit".into());
		}
	}

	Ok((Vec::from(&input[0..4]), input.slice(4..)))
}

// `ls32 = ( h16 ":" h16 ) / IPv4address`
fn parse_ls32(input: Bytes) -> ParseResult<Vec<u8>> {
	let p = alt!(
		seq!(c => {
			let mut bytes = vec![];
			bytes.extend(c.next(parse_h16)?.into_iter());
			c.next(one_of(":"))?;
			bytes.extend(c.next(parse_h16)?.into_iter());
			Ok(bytes)
		}),
		parse_ipv4_address
	);

	p(input)
}

// `IPv4address = dec-octet "." dec-octet "." dec-octet "." dec-octet`
fn parse_ipv4_address(input: Bytes) -> ParseResult<Vec<u8>> {
	let p = seq!(c => {
		let a1 = c.next(parse_dec_octet)?;
		c.next(one_of("."))?;
		let a2 = c.next(parse_dec_octet)?;
		c.next(one_of("."))?;
		let a3 = c.next(parse_dec_octet)?;
		c.next(one_of("."))?;
		let a4 = c.next(parse_dec_octet)?;
		Ok(vec![a1, a2, a3, a4])
	});

	p(input)
}


// `dec-octet = DIGIT                 ; 0-9
// 			  / %x31-39 DIGIT         ; 10-99
// 			  / "1" 2DIGIT            ; 100-199
// 			  / "2" %x30-34 DIGIT     ; 200-249
// 			  / "25" %x30-35          ; 250-255`
fn parse_dec_octet(input: Bytes) -> ParseResult<u8> {
	// TODO: Validate only taking 3 characters.
	let (u, rest) = take_while1(|i: u8| (i as char).is_digit(10))(input)?;
	let s = std::str::from_utf8(&u).map_err(|e| e.to_string())?;
	let v = u8::from_str_radix(s, 10).map_err(|e| e.to_string())?;
	Ok((v, rest))
}

// NOTE: This is strictly ASCII.
// `reg-name = *( unreserved / pct-encoded / sub-delims )`
parser!(parse_reg_name<AsciiString> => {
	map(many(alt!(
		parse_unreserved, parse_pct_encoded, parse_sub_delims
	)), |s| unsafe { AsciiString::from_ascii_unchecked(Bytes::from(s)) })
});

// `path = path-abempty    ; begins with "/" or is empty
// 		 / path-absolute   ; begins with "/" but not "//"
// 		 / path-noscheme   ; begins with a non-colon segment
// 		 / path-rootless   ; begins with a segment
// 		 / path-empty      ; zero characters`
parser!(parse_path<Path> => {
	alt!(
		map(parse_path_abempty, |s| Path::PathAbEmpty(s)),
		map(parse_path_absolute, |s| Path::PathAbsolute(s)),
		map(parse_path_noscheme, |s| Path::PathNoScheme(s)),
		map(parse_path_rootless, |s| Path::PathRootless(s)),
		map(parse_path_empty, |_| Path::PathEmpty)
	)
});

// NOTE: This is strictly ASCII.
// `path-abempty = *( "/" segment )`
parser!(parse_path_abempty<Vec<AsciiString>> => {
	many(seq!(c => {
		c.next(one_of("/"))?; // TODO
		c.next(parse_segment)
	}))
});

// NOTE: This is strictly ASCII.
// `path-absolute = "/" [ segment-nz *( "/" segment ) ]`
parser!(parse_path_absolute<Vec<AsciiString>> => {
	seq!(c => {
		c.next(one_of("/"))?; // TODO
		c.next(parse_path_rootless)
	})
});

// NOTE: This is strictly ASCII.
// `path-noscheme = segment-nz-nc *( "/" segment )`
parser!(parse_path_noscheme<Vec<AsciiString>> => {
	seq!(c => {
		let first_seg = c.next(parse_segment_nz_nc)?;
		let next_segs = c.next(many(seq!(c => {
			c.next(one_of("/"))?;
			c.next(parse_segment)
		})))?;

		let mut segs = vec![];
		segs.push(first_seg);
		segs.extend(next_segs.into_iter());
		Ok(segs)
	})
});

// NOTE: This is strictly ASCII.
// `path-rootless = segment-nz *( "/" segment )`
parser!(parse_path_rootless<Vec<AsciiString>> => {
	seq!(c => {
		let first_seg = c.next(parse_segment_nz)?;
		let next_segs = c.next(many(seq!(c => {
			c.next(one_of("/"))?;
			c.next(parse_segment)
		})))?;

		let mut segs = vec![];
		segs.push(first_seg);
		segs.extend(next_segs.into_iter());
		Ok(segs)
	})
});

// NOTE: This is strictly ASCII.
// `path-empty = 0<pchar>`
fn parse_path_empty(input: Bytes) -> ParseResult<()> {
	Ok(((), input.clone()))
}

// NOTE: This is strictly ASCII.
// `segment = *pchar`
parser!(parse_segment<AsciiString> => {
	map(many(parse_pchar),
		|s| unsafe { AsciiString::from_ascii_unchecked(Bytes::from(s)) })
});

// NOTE: This is strictly ASCII.
// `segment-nz = 1*pchar`
parser!(parse_segment_nz<AsciiString> => {
	map(many1(parse_pchar),
		|s| unsafe { AsciiString::from_ascii_unchecked(Bytes::from(s)) })
});


// NOTE: This is strictly ASCII.
// `segment-nz-nc = 1*( unreserved / pct-encoded / sub-delims / "@" )
// 				; non-zero-length segment without any colon ":"`
fn parse_segment_nz_nc(input: Bytes) -> ParseResult<AsciiString> {
	let p = map(many1(alt!(
		parse_unreserved, parse_pct_encoded, parse_sub_delims, one_of("@")
	)), |s| unsafe { AsciiString::from_ascii_unchecked(Bytes::from(s)) });

	p(input)
}

// NOTE: This is strictly ASCII.
// `pchar = unreserved / pct-encoded / sub-delims / ":" / "@"`
parser!(parse_pchar<u8> => {
	alt!(
		parse_unreserved, parse_pct_encoded, parse_sub_delims, one_of(":@")
	)
});

// NOTE: This is strictly ASCII.
// `query = *( pchar / "/" / "?" )`
parser!(parse_query<AsciiString> => parse_fragment);

// NOTE: This is strictly ASCII.
// `fragment = *( pchar / "/" / "?" )`
parser!(parse_fragment<AsciiString> => {
	map(many(alt!(
		parse_pchar, one_of("/?")
	)), |s| unsafe { AsciiString::from_ascii_unchecked(Bytes::from(s)) })
});

// NOTE: This is strictly ASCII.
// `pct-encoded = "%" HEXDIG HEXDIG`
fn parse_pct_encoded(input: Bytes) -> ParseResult<u8> {
	if input.len() < 3 || input[0] != ('%' as u8) {
		return Err("pct-encoded failed".into());
	}

	let s = std::str::from_utf8(&input[1..3]).map_err(|e| e.to_string())?;
	let v = u8::from_str_radix(s, 16).map_err(|e| e.to_string())?;

	if v > 0x7f || v <= 0x1f {
		return Err(
			format!("Percent encoded byte outside ASCII range: 0x{:x}", v).into());
	}

	Ok((v, input.slice(3..)))
}

// NOTE: This is strictly ASCII.
// `unreserved = ALPHA / DIGIT / "-" / "." / "_" / "~"`
parser!(parse_unreserved<u8> => {
	like(|i| {
		(i as char).is_alphanumeric() || is_one_of("-._~", i)
	})
});

// NOTE: This is strictly ASCII.
// NOTE: These must be 'pct-encoded' when appearing in a segment.
// `reserved = gen-delims / sub-delims`
fn is_reserved(i: u8) -> bool { is_gen_delims(i) || is_sub_delims(i) }

// NOTE: This is strictly ASCII.
// `gen-delims = ":" / "/" / "?" / "#" / "[" / "]" / "@"`
fn is_gen_delims(i: u8) -> bool { is_one_of(":/?#[]@", i) }

fn is_sub_delims(i: u8) -> bool { is_one_of("!$&'()*+,;=", i) }

// NOTE: This is strictly ASCII.
// `sub-delims = "!" / "$" / "&" / "'" / "(" / ")"
// 	           / "*" / "+" / "," / ";" / "="`
parser!(parse_sub_delims<u8> => like(is_sub_delims));

