use common::errors::*;

use crate::body::*;
use crate::chunked::*;
use crate::reader::*;

pub struct TransferCoding {
    pub raw_name: String,
    pub params: Vec<(String, String)>,
}

impl TransferCoding {
    pub fn name(&self) -> String {
        self.raw_name.to_ascii_lowercase()
    }
}

pub fn get_transfer_encoding_body(
    mut transfer_encoding: Vec<TransferCoding>,
    stream: StreamReader,
) -> Result<Box<dyn Body>> {
    let body: Box<dyn Body> = if transfer_encoding.last().unwrap().name() == "chunked" {
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
            // NOTE: Chunked is not handled here as it is only allow to occur once at the
            // end (which is handled above).
            return Err(format_err!("Unknown coding: {}", coding.name()));
        }
    }

    return Ok(body);
}
