// Utilities for reading/writing the length prefixed message framing format.

use std::io::Cursor;

use common::bytes::Bytes;
use common::errors::*;
use common::io::Readable;
use http::{Body, Headers};

const MESSAGE_HEADER_SIZE: usize = 5;

pub struct MessageReader<'a> {
    body: &'a mut dyn Body
}

impl<'a> MessageReader<'a> {
    pub fn new(body: &mut dyn Body) -> MessageReader {
        MessageReader { body }
    }

    pub async fn read(&mut self) -> Result<Option<Bytes>> {
        let mut header = [0u8; MESSAGE_HEADER_SIZE]; // Compressed flag + size.
        if let Err(e) = self.body.read_exact(&mut header).await {
            if let Some(io_error) = e.downcast_ref::<std::io::Error>() {
                if io_error.kind() == std::io::ErrorKind::UnexpectedEof {
                    return Ok(None);
                }
            }

            return Err(e);
        }

        if header[0] != 0 {
            return Err(err_msg("Decoding compressed messages not supported"));
        }

        // TODO: Need to enforce some reasonable limits on max size.
        let size = u32::from_be_bytes(*array_ref![header, 1, 4]) as usize;

        let mut data = vec![];
        data.reserve_exact(size);
        data.resize(size, 0);
        self.body.read_exact(&mut data).await?;

        Ok(Some(data.into()))
    }
}

pub struct UnaryMessageBody {
    len: usize,
    data: Cursor<Bytes>
}

impl UnaryMessageBody {
    pub fn new(data: Bytes) -> Box<dyn Body> {
        // TODO: Optimize this for the uncompressed case.

        let mut full_body = vec![];
        full_body.resize(MESSAGE_HEADER_SIZE, 0);
        *array_mut_ref![&mut full_body, 1, 4] = (data.len() as u32).to_be_bytes();

        full_body.extend_from_slice(&data);

        http::BodyFromData(full_body)


        // Box::new(Self {
        //     len: data.len(),
        //     data: Cursor::new(data),
        // })
    }
}

#[async_trait]
impl Body for UnaryMessageBody {
    fn len(&self) -> Option<usize> { Some(self.len + MESSAGE_HEADER_SIZE) }
    async fn trailers(&mut self) -> Result<Option<Headers>> { Ok(None) }    
}

#[async_trait]
impl Readable for UnaryMessageBody {
    async fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        self.data.read(buf).await
    }
}


/*
Challenges of sending non-unary messages:
- Need an Outgoing body implementation which allows limits us to the HTTP2 buffer size.
  Would be ideally be a little bit simpler 
*/