use crate::header_parser::*;
use crate::body::*;
use crate::chunked::*;
use crate::reader::*;
use common::errors::*;


pub fn get_transfer_encoding_body(mut transfer_encoding: Vec<TransferCoding>,
								  stream: StreamReader)
-> Result<Box<dyn Body>> {

	let mut body: Box<dyn Body> =
		if transfer_encoding.last().unwrap().name() == "chunked" {
			transfer_encoding.pop();
			Box::new(IncomingChunkedBody::new(stream))
		} else {
			Box::new(IncomingUnboundedBody { stream })
		};

	for coding in transfer_encoding.iter().rev() {
		if coding.name() == "identity" {
			continue;
		} else if coding.name() == "gzip" {

		} else if coding.name() == "deflate" {

		} else if coding.name() == "compress" {

		} else {
			// NOTE: Chunked is not handled here as it is only allow to occur once at the end (which is handled above).
			return Err(format!("Unknown coding: {}", coding.name()).into());
		}
	}

	return Ok(body);
}