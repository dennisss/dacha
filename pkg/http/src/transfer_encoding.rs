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


// TODO: Move these to another file?

struct DeflateBody {
    body: Box<dyn Body>,
    deflater: compression::deflate::Deflater
}

impl DeflateBody {
    
}

#[async_trait]
impl Body for DeflateBody {
    fn len(&self) -> Option<usize> { self.body.len() }

    async fn read(&mut self, buf: &mut [u8]) -> Result<usize> {

        let mut buf = [0u8; 512];


        // Try to read from the inner 

        // self.deflater.update(mut input: &[u8], mut output: &mut [u8], is_final: bool)

        // std::io::Read::read(self, buf).map_err(|e| Error::from(e))
    }
}


struct InflateBody {
    body: Box<dyn Body>,
    inflater: compression::deflate::Inflater;
}

#[async_trait]
impl Body for InflateBody {
    fn len(&self) -> Option<usize> { self.body.len() }

    async fn read(&mut self, buf: &mut [u8]) -> Result<usize> {

        let mut buf = [0u8; 512];
        
        // TODO: It is possible that not all of the 

        // Try to read from the inner 

        // self.deflater.update(mut input: &[u8], mut output: &mut [u8], is_final: bool)

        // std::io::Read::read(self, buf).map_err(|e| Error::from(e))
    }
}


struct GZipBody {
    body: Box<dyn Body>
}


struct GZipReader {
    body: Box<dyn Body>,
}

/*
impl Read for GZipReader {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<()> {
        // let gz = read_gzip(&mut f)?;
        // println!("{:?}", gz);

        // // TODO: Don't allow reading beyond end of range
        // f.seek(std::io::SeekFrom::Start(gz.compressed_range.0))?;

        // // Next step is to validate the CRC and decompressed size?
        // // Also must implement as an incremental state machine using async/awaits!

        // read_inflate(&mut f)?;

    }
}
*/