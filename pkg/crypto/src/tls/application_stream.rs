use common::errors::*;
use common::io::{Readable, Writeable};
use common::bytes::{Bytes, Buf};
use common::async_std::sync::Mutex;

use crate::tls::record_stream::*;

/// Abstraction around the raw TLS record layer for reading/writing application data.
/// It buffers read bytes so that it can be used as a Readable / Writeable stream.
pub struct ApplicationStream {
    record_stream: RecordStream,
    read_buffer: Mutex<Bytes>,
}

impl ApplicationStream {
    pub(crate) fn new(record_stream: RecordStream) -> Self {
        Self {
            record_stream,
            read_buffer: Mutex::new(Bytes::new())
        }
    }
}

#[async_trait]
impl Readable for ApplicationStream {
    async fn read(&mut self, mut buf: &mut [u8]) -> Result<usize> {
        // TODO: We should dedup this with the http::Body code.
        let mut read_buffer = self.read_buffer.lock().await;
        let mut nread = 0;
        if read_buffer.len() > 0 {
            let n = std::cmp::min(buf.len(), read_buffer.len());
            buf[0..n].copy_from_slice(&read_buffer[0..n]);
            read_buffer.advance(n);
            buf = &mut buf[n..];
            nread += n;
        }

        if buf.len() == 0 {
            return Ok(nread);
        }

        let msg = self.record_stream.recv(None).await?;
        if let Message::ApplicationData(mut data) = msg {
            let n = std::cmp::min(data.len(), buf.len());
            buf[0..n].copy_from_slice(&data[0..n]);
            nread += n;
            data.advance(n);

            *read_buffer = data;

            Ok(nread)
        } else {
            // TODO: Now in an error state. Future reads should fail?
            Err(err_msg("Unexpected data seen on stream"))
        }
    }
}

#[async_trait]
impl Writeable for ApplicationStream {
    async fn write(&mut self, buf: &[u8]) -> Result<usize> {
        // TODO: We may need to split up a packet that is too large.
        self.record_stream.send(buf).await?;
        Ok(buf.len())
    }

    async fn flush(&mut self) -> Result<()> {
        self.record_stream.flush().await?;
        Ok(())
    }
}