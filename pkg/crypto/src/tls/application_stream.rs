use std::sync::atomic::AtomicUsize;
use std::sync::Arc;

use common::errors::*;
use common::io::{Readable, Writeable};
use common::bytes::{Bytes, Buf};

use crate::tls::alert::*;
use crate::tls::handshake::Handshake;
use crate::tls::record_stream::*;
use crate::tls::handshake_summary::HandshakeSummary;

// TODO: Must mention that these interfaces must be constantly polled for the connection to stay healthy?

/// Abstraction around the raw TLS record layer for reading/writing application data.
/// It buffers read bytes so that it can be used as a Readable / Writeable stream.
pub struct ApplicationStream {
    pub reader: ApplicationDataReader,
    pub writer: ApplicationDataWriter,
    pub handshake_summary: HandshakeSummary
}

impl ApplicationStream {
    pub(crate) fn new(
        record_reader: RecordReader, record_writer: RecordWriter,
        handshake_summary: HandshakeSummary
    ) -> Self {
        let pending_key_updates = Arc::new(AtomicUsize::new(0));

        Self {
            reader: ApplicationDataReader {
                record_reader,
                read_buffer: Bytes::new(),
                pending_key_updates: pending_key_updates.clone()
            },
            writer: ApplicationDataWriter {
                record_writer,
                pending_key_updates
            },
            handshake_summary
        }
    }
}

pub struct ApplicationDataReader {
    record_reader: RecordReader,

    /// Data which has been received in the most recent packet but hasn't been
    /// read by the downstream protocol reading from this stream.
    read_buffer: Bytes,

    /// Number of unacknowledged KeyUpdate messages which effect our local  
    pending_key_updates: Arc<AtomicUsize>
}


#[async_trait]
impl Readable for ApplicationDataReader {
    async fn read(&mut self, mut buf: &mut [u8]) -> Result<usize> {
        // TODO: We should dedup this with the http::Body code.
        let mut nread = 0;
        if self.read_buffer.len() > 0 {
            let n = std::cmp::min(buf.len(), self.read_buffer.len());
            buf[0..n].copy_from_slice(&self.read_buffer[0..n]);
            self.read_buffer.advance(n);
            buf = &mut buf[n..];
            nread += n;
        }

        if buf.len() == 0 {
            return Ok(nread);
        }

        loop {
            let msg = self.record_reader.recv(None).await?;
            if let Message::ApplicationData(mut data) = msg {
                let n = std::cmp::min(data.len(), buf.len());
                buf[0..n].copy_from_slice(&data[0..n]);
                nread += n;
                data.advance(n);

                self.read_buffer = data;

                return Ok(nread)
            } else if let Message::Handshake(Handshake::NewSessionTicket(_)) = msg {
                println!("IGNORING NEW SESSION TICKET");
                continue;
            } else if let Message::Alert(alert) = msg {
                if alert.level == AlertLevel::fatal {
                    // TODO: In this case, close the underlying stream?
                    return Err(err_msg("Fatal TLS error received"));
                }

                if alert.description == AlertDescription::close_notify {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::ConnectionAborted, "TLS close_notify received").into());
                }
                // TODO: Fatal errors should 

                println!("ALERT: {:?}", alert);

            } else {
                println!("{:?}", msg);
                // TODO: Now in an error state. Future reads should fail?
                return Err(err_msg("Unexpected data seen on stream"));
            }
        }
    }
}

pub struct ApplicationDataWriter {
    record_writer: RecordWriter,

    pending_key_updates: Arc<AtomicUsize>
}

#[async_trait]
impl Writeable for ApplicationDataWriter {
    async fn write(&mut self, buf: &[u8]) -> Result<usize> {
        // TODO: Update keys 

        // TODO: We may need to split up a packet that is too large.
        self.record_writer.send(buf).await?;
        Ok(buf.len())


        // TODO: Need to implement a close that sends a close_notify.
    }

    async fn flush(&mut self) -> Result<()> {
        self.record_writer.flush().await?;
        Ok(())
    }
}