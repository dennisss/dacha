use common::bits::{BitOrder, BitVector, BitWrite, BitWriter};
use common::errors::*;
use compression::buffer_queue::BufferQueue;
use compression::deflate::cyclic_buffer::CyclicBuffer;
use compression::snappy::window::*;
use compression::transform::{Transform, TransformProgress};

use crate::{constants::*, Options};

// TODO: Current limitation is that we can only encode back references of at
// least 4 bytes in length. (the reference implementation supports >= 2 and will
// decide the minimum based on whether or not it is worth it with encoding
// overheads.)

/// NOTE: Compression will be suboptimal if the input is provided in small
/// chunks since we currently do not check for backreferences that span across
/// input chunk boundaries.
pub struct Encoder {
    options: Options,
    window: MatchingWindowSnappy<CyclicBuffer>,
    output_buffer: BufferQueue,
    output_buffer_end: BitVector,
}

impl Encoder {
    pub fn new(options: Options) -> Result<Self> {
        options.validate()?;

        let window_size = 1 << options.window_bits;
        let lookahead_max = 1 << options.lookahead_bits;

        let window = MatchingWindowSnappy::new(
            CyclicBuffer::new(window_size),
            MatchingWindowSnappyOptions {
                table_size: window_size, // NOTE: Doesn't need to be the same as the window size.
                max_match_length: lookahead_max,
            },
        );

        Ok(Self {
            options,
            window,
            output_buffer: BufferQueue::new(),
            output_buffer_end: BitVector::new(),
        })
    }
}

impl Transform for Encoder {
    fn update(
        &mut self,
        input: &[u8],
        end_of_input: bool,
        output: &mut [u8],
    ) -> Result<TransformProgress> {
        let mut strm =
            BitWriter::new_with_order(&mut self.output_buffer.buffer, BitOrder::MSBFirst);
        strm.write_bitvec(&self.output_buffer_end)?;

        let mut i = 0;
        while i < input.len() {
            if let Some(m) = self.window.find_match(&input[i..]) {
                strm.write_bits(BACKREFERENCE_TAG, 1)?;
                strm.write_bits(m.distance - 1, self.options.window_bits as u8)?;
                strm.write_bits(m.length - 1, self.options.lookahead_bits as u8)?;
                self.window.advance(&input[i..(i + m.length)]);
                i += m.length;
                continue;
            }

            // Uncompressed Literal
            strm.write_bits(LITERAL_TAG, 1)?;
            strm.write_bits(input[i] as usize, 8)?;
            self.window.advance(&input[i..(i + 1)]);
            i += 1;
        }

        if end_of_input {
            strm.finish()?;
        }

        self.output_buffer_end = strm.into_bits();

        let output_written = self.output_buffer.copy_to(output);

        Ok(TransformProgress {
            input_read: i,
            output_written,
            done: self.output_buffer_end.len() == 0 && self.output_buffer.is_empty(),
            event: (),
        })
    }
}
