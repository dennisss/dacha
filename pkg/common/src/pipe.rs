// Implementation of an in-memory thread safe pipe.
//
// Simply call 'let (w, r) = pipe();' then any data written to 'w' will be
// available to be read at some point in the future via 'r'. Data is internally
// buffered up to a limit so the writer may block if the reader isn't reading
// fast enough.

use std::sync::Arc;

use async_std::channel;
use async_std::sync::Mutex;

use crate::errors::*;
use crate::io::{Readable, Writeable};

pub struct PipeWriter {
    buffer: Arc<Mutex<Vec<u8>>>,
    notifier: channel::Sender<()>,
    waiter: channel::Receiver<()>,
}

#[async_trait]
impl Writeable for PipeWriter {
    async fn write(&mut self, buf: &[u8]) -> Result<usize> {
        const MAX_BUFFER_SIZE: usize = 4096;

        loop {
            {
                let mut buffer = self.buffer.lock().await;
                if buffer.len() < MAX_BUFFER_SIZE {
                    let n = std::cmp::min(MAX_BUFFER_SIZE - buffer.len(), buf.len());
                    buffer.extend_from_slice(&buf[0..n]);

                    let _ = self.notifier.try_send(());

                    return Ok(n);
                }
            }

            if let Err(_) = self.waiter.recv().await {
                return Err(err_msg("Reader hung up"));
            }
        }
    }

    async fn flush(&mut self) -> Result<()> {
        Ok(())
    }
}

pub struct PipeReader {
    buffer: Arc<Mutex<Vec<u8>>>,
    notifier: channel::Sender<()>,
    waiter: channel::Receiver<()>,
}

#[async_trait]
impl Readable for PipeReader {
    async fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        loop {
            {
                let mut buffer = self.buffer.lock().await;
                if !buffer.is_empty() {
                    let n = std::cmp::min(buf.len(), buffer.len());
                    &buf[0..n].copy_from_slice(&buffer[0..n]);

                    // Remove first 'n' bytes from the buffer.
                    let new_len = buffer.len() - n;
                    for i in 0..new_len {
                        buffer[i] = buffer[i + n];
                    }
                    buffer.truncate(new_len);

                    let _ = self.notifier.try_send(());

                    return Ok(n);
                }
            }

            if let Err(_) = self.waiter.recv().await {
                // Other side is closed.
                return Ok(0);
            }
        }
    }
}

pub fn pipe() -> (PipeWriter, PipeReader) {
    let (writer_notifier, writer_waiter) = channel::bounded(1);
    let (reader_notifier, reader_waiter) = channel::bounded(1);

    let buffer = Arc::new(Mutex::new(vec![]));

    let writer = PipeWriter {
        buffer: buffer.clone(),
        notifier: reader_notifier,
        waiter: writer_waiter,
    };

    let reader = PipeReader {
        buffer,
        notifier: writer_notifier,
        waiter: reader_waiter,
    };

    (writer, reader)
}
