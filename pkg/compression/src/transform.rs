use common::errors::Result;

/// Applies some operation to a stream of input byes in order to produce a stream of output bytes.
/// 
/// Transforms are allowed to be stateful, but should NOT have any asynchronous or
/// non-deterministic behaviors. Rather it is expected that the transform is completely CPU
/// intensive.
/// 
/// Also note some algorithms only have authentication/checksums validated once all inputs have
/// been read / all outputs have been written. So a corrupt stream may only return an error at
/// the very end. Keep this in mind if doing anything security critical and prefer not to use
/// the output 
pub trait Transform {
    /// Applies the operation on one chunk of input/output buffers.
    ///
    /// Unless an error occured, the transform is expected to make as much progress as possible.
    /// This means that:
    /// - When this returns, the input buffer and/or the output buffer will be completely used or
    ///   done will be set to true.
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

    /// If true, then all output has been written.
    /// 
    /// NOTE: This is allowed to be true even before all inputs have been read. In this case the
    /// transform likely hit an end of stream marker as defined by the transform's algorithm. 
    pub done: bool,

    // TODO: Allow outputing a remaining_output_length hint
}

/// Helper that consumes all of the input data and transforms it into the output vector.
/// 
/// Returns the number of input bytes read. Number of output bytes should be trivial. But, the question is whether or 
pub fn transform_to_vec(
    transform: &mut dyn Transform, mut input: &[u8], end_of_input: bool,
    output: &mut Vec<u8>
) -> Result<TransformProgress> {
    const CHUNK_SIZE: usize = 512;

    let mut input_read = 0;
    let mut output_len = output.len();
    let original_output_len = output.len();
    let mut final_done = false;
    
    loop {
        // Always use all data that we already allocated. If the called already knew the length of the output, they could
        // reserve that size to perform a one shot transformation that doesn't require re-allocating.
        output.resize(output.capacity(), 0);

        let progress = transform.update(input, end_of_input, &mut output[output_len..])?;
        input = &input[progress.input_read..];

        input_read += progress.input_read;
        output_len += progress.output_written;
        final_done = progress.done;

        if progress.done || input.len() == 0 && progress.output_written < CHUNK_SIZE {
            break;
        }

        output.reserve(CHUNK_SIZE);
    }

    output.truncate(output_len);
    
    Ok(TransformProgress {
        input_read,
        output_written: (output_len - original_output_len),
        done: final_done
    })
}
