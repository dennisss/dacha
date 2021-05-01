// Helpers to work with the Content-Encoding, Transfer-Encoding, and Accept-Encoding headers.
//
// The definitive list of registered coding types is here:
// https://www.iana.org/assignments/http-parameters/http-parameters.xml#http-parameters-1

use common::errors::*;
use compression::gzip::GzipDecoder;
use compression::deflate::Inflater;

// Transfer-Encoding: https://tools.ietf.org/html/rfc7230#section-3.3.1


use crate::body::*;
use crate::chunked::*;
use crate::reader::*;
use crate::header::Headers;

pub struct TransferCoding {
    pub raw_name: String,

    // TODO: Do something with these? (at least check that they are empty)
    pub params: Vec<(String, String)>,
}

impl TransferCoding {
    pub fn name(&self) -> String {
        self.raw_name.to_ascii_lowercase()
    }
}

// TODO: When encoding, we need to check TE 

pub fn get_transfer_encoding_body(
    mut transfer_encoding: Vec<TransferCoding>,
    stream: StreamReader,
) -> Result<Box<dyn Body>> {

    // TODO: 'chunked' is allowed to not be first for responses?
    let mut body: Box<dyn Body> = if transfer_encoding.last().unwrap().name() == "chunked" {
        transfer_encoding.pop();
        Box::new(IncomingChunkedBody::new(stream))
    } else {
        Box::new(IncomingUnboundedBody { stream })
    };

    for coding in transfer_encoding.iter().rev() {
        if coding.name() == "identity" {
            continue;
        } else if coding.name() == "gzip" || coding.name() == "x-gzip" {
            body = Box::new(TransformBody::new(body, Box::new(GzipDecoder::new())));
        } else if coding.name() == "deflate" {
            body = Box::new(TransformBody::new(body, Box::new(Inflater::new())));
        } else {
            // TODO: Support "compress"

            // NOTE: Chunked is not handled here as it is only allow to occur once at the
            // end (which is handled above).
            return Err(format_err!("Unknown coding: {}", coding.name()));
        }
    }

    return Ok(body);
}

pub fn decode_content_encoding_body(
    response_headers: &Headers,
    mut body: Box<dyn Body>
) -> Result<Box<dyn Body>> {
    let codings = crate::encoding_syntax::parse_content_encoding(response_headers)?;

    for coding in codings.iter().rev() {
        if coding == "identity" {
            continue;
        } else if coding == "gzip" || coding == "x-gzip" {
            body = Box::new(TransformBody::new(body, Box::new(GzipDecoder::new())));
        } else if coding == "deflate" {
            body = Box::new(TransformBody::new(body, Box::new(Inflater::new())));
        } else {
            // TODO: Support "compress"

            return Err(format_err!("Unknown coding: {}", coding));
        }
    }

    Ok(body)
}

// fn decode_body(body: Box<dyn Body>, coding_name: ) ->
