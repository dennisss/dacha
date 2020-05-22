use common::errors::*;
use crate::spec::*;
use crate::header::*;
use bytes::Bytes;
use parsing::*;
use crate::common_parser::*;
use parsing::ascii::AsciiString;

// See https://tools.ietf.org/html/rfc7230#section-3.3.1


/// NOTE: Names are case insensitive

pub struct TransferCoding {
	name: String,
	pub params: Vec<(String, String)>
}

impl TransferCoding {
	pub fn name(&self) -> String {
		self.name.to_ascii_lowercase()
	}

	pub fn raw_name(&self) -> &str {
		&self.name
	}
}

// TODO: What to do about empty elements again?

// TODO: Must also ignore empty list items especially in the 1#element case
// ^ Parsing too many empty values may result in denial of service

/// Encodes http comma separated values as used in headers.
/// It is recommended to always explicitly define a maximum number of items.
/// 
/// Per RFC7230, this will tolerate empty items.
/// 
/// In the RFCs, this corresponds to these grammar rules:
/// `1#element => element *( OWS "," OWS element )`
/// `#element => [ 1#element ]`
/// `<n>#<m>element => element <n-1>*<m-1>( OWS "," OWS element )`
fn comma_delimited<T, P: Parser<T> + Copy>(p: P, min: usize, max: usize)
	-> impl Parser<Vec<T>> {
	assert!(max >= min && max >= 1);

	seq!(c => {
		let mut out = vec![];
		out.push(c.next(p)?);
		while out.len() < max {
			let item = c.next(opt(seq!(c => {
				c.next(parse_ows)?;
				c.next(one_of(","))?;
				c.next(parse_ows)?;
				Ok(c.next(opt(p))?)
			})))?;

			match item {
				Some(Some(v)) => out.push(v),
				// In this case, we failed to parse (meaning most likely the
				// item is empty and will be skipped in the next loop).
				// NOTE: That behavior assumes that this is wrapped by a
				// complete()
				Some(None) => { continue; }
				None => { break; }
			};
		}

		if out.len() < min {
			return Err(err_msg("Too few items parsed"));
		}

		Ok(out)
	})
}

// `transfer-parameter = token BWS "=" BWS ( token / quoted-string )`
parser!(parse_transfer_parameter<(String, String)> => {
	seq!(c => {
		let name = c.next(parse_token)?.to_string();
		c.next(parse_bws)?;
		c.next(one_of("="))?;
		c.next(parse_bws)?;
		let value = c.next(alt!(
			map(parse_token, |s| s.to_string()),
			map(parse_quoted_string, |s| s.to_string())
		))?;
		Ok((name, value))
	})
});

// `transfer-extension = token *( OWS ";" OWS transfer-parameter )`
parser!(parse_transfer_extension<TransferCoding> => {
	seq!(c => {
		let name = c.next(parse_token)?.to_string();
		let params = c.many(seq!(c => {
			c.next(parse_ows)?;
			c.next(one_of(";"))?;
			c.next(parse_ows)?;
			Ok(c.next(parse_transfer_parameter)?)
		}));

		Ok(TransferCoding {
			name, params
		})
	})
});

// `transfer-coding = "chunked" / "compress"
//					/ "deflate" / "gzip" / transfer-extension`
parser!(parse_transfer_coding<TransferCoding> => {
	parse_transfer_extension
});

pub const MAX_TRANSFER_CODINGS: usize = 4;

// TODO: Must tolerate empty items in comma delimited lsit 
pub fn parse_transfer_encoding(headers: &HttpHeaders)
-> Result<Vec<TransferCoding>> {
	let mut out = vec![];
	for h in headers.find(TRANSFER_ENCODING) {
		let (items, _) =
			complete(comma_delimited(parse_transfer_coding, 1, MAX_TRANSFER_CODINGS))(h.value.data.clone())?;

		out.reserve(items.len());
		for i in items.into_iter() {
			out.push(i);
		}

		if out.len() > MAX_TRANSFER_CODINGS {
			return Err(err_msg("Too many Transfer-Codings"))
		}
	}

	Ok(out)
}


// A sender MUST
//    NOT apply chunked more than once to a message body (i.e., chunking an
//    already chunked message is not allowed).  If any transfer coding
//    other than chunked is applied to a request payload body, the sender
//    MUST apply chunked as the final transfer coding to ensure that the
//    message is properly framed.


// `Content-Length = 1*DIGIT`
pub fn parse_content_length(headers: &HttpHeaders) -> Result<Option<usize>> {
	let mut hs = headers.find(CONTENT_LENGTH);
	let len = if let Some(h) = hs.next() {
		if let Ok(v) = usize::from_str_radix(&h.value.to_string(), 10) {
			Some(v)
		} else {
			return Err(format_err!("Invalid Content-Length: {:?}", h.value));
		}
	} else {
		// No header present.
		None
	};

	// Having more than one header is an error.
	if !hs.next().is_none() {
		return Err(err_msg("More than one Content-Length header received."));
	}

	Ok(len)
}

parser!(parse_content_coding<AsciiString> => parse_token);

const MAX_CONTENT_ENCODINGS: usize = 4;

/// This will return a list of all content encodings in the message.
/// For simplicity they will all be lowercased as 
pub fn parse_content_encoding(headers: &HttpHeaders) -> Result<Vec<String>> {
	// TODO: Deduplicate this code with parse_transfer_encoding
	let mut out = vec![];
	for h in headers.find(CONTENT_ENCODING) {
		let (items, _) =
			complete(comma_delimited(parse_content_coding, 1, MAX_CONTENT_ENCODINGS))(h.value.data.clone())?;

		out.reserve(items.len());
		for i in items.into_iter() {
			out.push(i.to_string().to_ascii_lowercase());
		}

		if out.len() > MAX_CONTENT_ENCODINGS {
			return Err(err_msg("Too many Transfer-Codings"))
		}
	}

	Ok(out)
}

pub struct Parameter {
	pub name: String,
	// NOTE: Case sentitive detepending on the parameter name.
	pub value: String
}

parser!(parse_parameter<Parameter> => {
	seq!(c => {
		let name = c.next(parse_token)?.to_string().to_ascii_lowercase();
		c.next(one_of("="))?;
		let value = c.next(alt!(
			map(parse_token, |t| t.to_string()),
			map(parse_quoted_string, |t| t.to_string())
		))?;
		Ok(Parameter { name, value })
	})
});


// NOTE: All of this is case-insensitive and will be stored in ascii lower case.
pub struct MediaType {
	pub typ: String,
	pub subtype: String,
	pub params: Vec<Parameter>
} 

parser!(parse_media_type<MediaType> => {
	seq!(c => {
		let typ = c.next(parse_token)?.to_string().to_ascii_lowercase();
		c.next(one_of("/"))?;
		let subtype = c.next(parse_token)?.to_string().to_ascii_lowercase();
		let params = c.many(seq!(c => {
			c.next(parse_ows)?;
			c.next(one_of(";"))?;
			c.next(parse_ows)?;
			c.next(parse_parameter)
		}));

		Ok(MediaType {
			typ, subtype, params
		})
	})
});

pub fn parse_content_type(headers: &HttpHeaders) -> Result<Option<MediaType>> {
	let mut h_iter = headers.find(CONTENT_TYPE);
	let h = if let Some(h) = h_iter.next() { h } else { return Ok(None); };
	if !h_iter.next().is_none() {
		return Err(err_msg("More than one content-type"));
	}

	let (typ, _) =
			complete(parse_media_type)(h.value.data.clone())?;
	Ok(Some(typ))
}

