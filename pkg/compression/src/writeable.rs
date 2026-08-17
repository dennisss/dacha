use common::errors::*;
use common::io::Writeable;

use crate::transform::Transform;

const OUTPUT_BUFFER_SIZE: usize = 8192;

// NOTE: This currently uses flush() to signal that all data has been written.
// TODO: Need better end_of_input flushing if this is dropped.
pub struct TransformWriteable<W: Writeable> {
    writer: W,

    /// The transform which is being applied.
    transform: Box<dyn Transform + Send + Sync>,

    /// Buffer
    output_buffer: Vec<u8>,

    done: bool,
}

impl<W: Writeable> TransformWriteable<W> {
    pub fn new(writer: W, transform: Box<dyn Transform + Send + Sync>) -> Self {
        Self {
            writer,
            transform,
            output_buffer: vec![],
            done: false
        }
    }

    // Consumes all input data and if it is the end of the input, also waits for
    // all data to be written.
    async fn update(&mut self, mut input_buf: &[u8], end_of_input: bool) -> Result<()> {
        while !input_buf.is_empty() || (end_of_input && !self.done) {
            self.output_buffer.resize(OUTPUT_BUFFER_SIZE, 0);
            let progress = self.transform.update(input_buf, end_of_input, &mut self.output_buffer)?;

            input_buf = &input_buf[progress.input_read..];

            let n = progress.output_written;
            self.writer.write_all(&self.output_buffer[0..n]).await?;

            if progress.done && (!input_buf.is_empty() || !end_of_input) {
                return Err(err_msg("Unexpected end of writing transform"));
            }

            self.done = progress.done;
        }

        Ok(())
    }
}

#[async_trait]
impl<W: Writeable> Writeable for TransformWriteable<W> {
    async fn write(&mut self, buf: &[u8]) -> Result<usize> {
        if self.done {
            return Err(err_msg("Writing extra data after flush"));
        }

        self.update(buf, false).await?;
        Ok(buf.len())
    }

    async fn flush(&mut self) -> Result<()> {
        self.update(&[], true).await?;
        self.writer.flush().await?;
        Ok(())
    }
}

