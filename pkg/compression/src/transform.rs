use common::errors::Result;

/// Applies some operation to a stream of input byes in order to produce a stream of output bytes.
/// 
/// Transforms are allowed to be stateful, but should NOT have any asynchronous or
/// non-deterministic behaviors. Rather it is expected that the transform is completely CPU
/// intensive.
pub trait Transform {
    /// Applies the operation on one chunk of input/output buffers.
    ///
    /// Unless an error occured, the transform is expected to make as much progress as possible.
    /// This means that:
    /// - When this returns, the input buffer and/or the output buffer will be completely used.
    /// - If the output buffer isn't completely used, then no more internally buffered output data
    ///   is available.
    fn update(&mut self, input: &[u8], end_of_input: bool, output: &mut [u8]) -> Result<TransformProgress>;
}

#[derive(Debug)]
pub struct TransformProgress {
    /// Number of input bytes consumed during the update.
    pub input_read: usize,

    /// Number of output bytes written into the given buffer during the update.
    pub output_written: usize,

    /// If true, then all output has been written
    pub done: bool,

    // TODO: Allow outputing a remaining_output_length hint
}

/// Helper that consumes all of the input data and transforms it into the output vector.
pub fn transform_to_vec(transform: &mut dyn Transform, mut input: &[u8], end_of_input: bool, output: &mut Vec<u8>) -> Result<()> {
    const CHUNK_SIZE: usize = 512;

    let mut output_len = output.len();
    loop {
        output.resize(output_len + CHUNK_SIZE, 0);

        let progress = transform.update(input, end_of_input, &mut output[output_len..])?;
        input = &input[progress.input_read..];

        output_len += progress.output_written;

        if input.len() == 0 && progress.output_written < CHUNK_SIZE {
            break;
        }
    }

    output.truncate(output_len);
    Ok(())
}

// Things that don't need to happen?
// - Don't need to consume all inputs if 