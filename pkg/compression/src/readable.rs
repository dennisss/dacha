use common::errors::*;
use common::io::Readable;

use crate::transform::Transform;

pub struct TransformReadable<R: Readable> {
    /// Input reader which we are transforming.
    reader: R,

    /// The transform which is being applied.
    transform: Box<dyn Transform + Send + Sync>,

    /// Data that has been read from the input body but hasn't been digested by
    /// the Transform.
    input_buffer: Vec<u8>,

    input_buffer_offset: usize,

    /// Whether or not we have read all of the input data yet.
    end_of_input: bool,

    /// Whether or not the transform is done (no more data will be outputted).
    end_of_output: bool,
}

impl<R: Readable> TransformReadable<R> {
    pub fn new(reader: R, transform: Box<dyn Transform + Send + Sync>) -> Self {
        let mut input_buffer = vec![];
        input_buffer.reserve_exact(512);

        Self {
            reader,
            transform,
            input_buffer,
            input_buffer_offset: 0,
            end_of_input: false,
            end_of_output: false,
        }
    }

    pub fn inner_reader(&self) -> &R {
        &self.reader
    }

    /// TODO: Consider getting rid of this as this may allow reading from the
    /// reader and disruipting the transformer's state.
    pub fn inner_reader_mut(&mut self) -> &mut R {
        &mut self.reader
    }
}

#[async_trait]
impl<R: Readable> Readable for TransformReadable<R> {
    async fn read(&mut self, mut output: &mut [u8]) -> Result<usize> {
        let mut output_written = 0;

        loop {
            // Trivially can't do anything in this case.
            // NOTE: end_of_input will always be set after end_of_output.
            if output.is_empty() || self.end_of_input {
                return Ok(output_written);
            }

            if !self.input_buffer.is_empty() {
                // TODO: attempt to execute this multiple times if no data was consumed.
                let progress = self.transform.update(
                    &self.input_buffer[self.input_buffer_offset..],
                    self.end_of_input,
                    output,
                )?;

                self.input_buffer_offset += progress.input_read;
                if self.input_buffer_offset == self.input_buffer.len() {
                    // All input data was consumed. Can clear the buffer.
                    self.input_buffer_offset = 0;
                    self.input_buffer.clear();
                }

                output_written += progress.output_written;
                output = &mut output[progress.output_written..];

                if progress.done {
                    self.end_of_output = true;
                    if !self.input_buffer.is_empty() {
                        return Err(err_msg("Remaining input data after end of output"));
                    }
                }

                if !self.input_buffer.is_empty() {
                    // Input data is remaining. Likely we ran out of space in the output buffer.
                    // NOTE: We won't read new data from the input body until all current data has
                    // been consumed.

                    if output_written == 0 {
                        return Err(err_msg("Transform made no progress"));
                    }

                    return Ok(output_written);
                }

                continue;
            }

            // Read more data into the input buffer.
            self.input_buffer.resize(self.input_buffer.capacity(), 0);
            let n = self.reader.read(&mut self.input_buffer).await?;
            self.input_buffer.truncate(n);

            if n == 0 {
                self.end_of_input = true;
                if !self.end_of_output {
                    return Err(err_msg("End of input seen before end of output"));
                }

                return Ok(output_written);
            }

            // We now have data in our buffer which will be transformed in the
            // next iteration of this loop.
        }
    }
}
