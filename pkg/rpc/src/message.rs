// Utilities for reading/writing the length prefixed message framing format.

use std::io::Cursor;

use common::bytes::Bytes;
use common::errors::*;
use common::io::{IoError, IoErrorKind, Readable};

const MESSAGE_HEADER_SIZE: usize = 5;

pub struct Message {
    pub data: Bytes,

    /// Whether or not this message contains trailer headers data.
    /// This should only be true for a server sending a client data using a GRPC
    /// web protocol.
    pub is_trailers: bool,
}

pub struct MessageReader<'a> {
    // TODO: Eventually change to use Readable instead of http::Body.
    reader: &'a mut dyn http::Body,
}

impl<'a> MessageReader<'a> {
    pub fn new(reader: &'a mut dyn http::Body) -> Self {
        Self { reader }
    }

    pub async fn read(&mut self) -> Result<Option<Message>> {
        let mut header = [0u8; MESSAGE_HEADER_SIZE]; // Compressed flag + size.
        if let Err(e) = self.reader.read_exact(&mut header).await {
            // If we read nothing, then there are no more messages left in the stream.
            if let Some(IoError {
                kind: IoErrorKind::UnexpectedEof { num_read: 0 },
                ..
            }) = e.downcast_ref()
            {
                return Ok(None);
            }

            return Err(e);
        }

        let is_trailers = header[0] & (1 << 7) != 0;
        let compression_flags = header[0] & ((1 << 7) - 1);

        if compression_flags != 0 {
            return Err(err_msg("Decoding compressed messages not supported"));
        }

        // TODO: Need to enforce some reasonable limits on max size.
        let size = u32::from_be_bytes(*array_ref![header, 1, 4]) as usize;

        let mut data = vec![];
        data.reserve_exact(size);
        data.resize(size, 0);
        self.reader.read_exact(&mut data).await?;

        Ok(Some(Message {
            data: data.into(),
            is_trailers,
        }))
    }
}

pub struct MessageSerializer {}

impl MessageSerializer {
    /// Serializes the header for a message containing 'data'.
    /// Assuming the data should stay uncompressed, then the message can be
    /// constructed as [header, data].
    pub fn serialize_header(data: &[u8], is_trailers: bool) -> Bytes {
        let mut output = vec![];
        output.resize(MESSAGE_HEADER_SIZE, 0);

        if is_trailers {
            output[0] = 1 << 7;
        }

        *array_mut_ref![output, 1, 4] = (data.len() as u32).to_be_bytes();

        output.into()
    }
}
