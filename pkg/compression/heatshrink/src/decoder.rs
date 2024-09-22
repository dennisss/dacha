use std::intrinsics::unlikely;

use common::bits::{BitIoError, BitOrder, BitReader, BitReaderRawState, BitVector};
use common::errors::*;
use compression::buffer_queue::BufferQueue;
use compression::deflate::cyclic_buffer::{CyclicBuffer, WindowBuffer};
use compression::snappy::window::*;
use compression::transform::{Transform, TransformProgress};

use crate::{constants::*, Options};

pub struct Decoder {
    options: Options,

    state: State,

    /// Remaining bits from the compressed input which we have consumed but have
    /// not processed yet.
    input_prefix: Option<BitReaderRawState>,

    /// Stores the last N bytes of uncompressed data produced.
    output_window: CyclicBuffer,
}

#[derive(Clone, Copy, PartialEq)]
enum State {
    /// Next bit is a tag.
    Start,

    /// We read a backreference from the input bitstream and we are currently
    Backreference { index: usize, len: usize },
}

// TODO: Dedup this.
struct OutputBuffer<'a> {
    buf: &'a mut [u8],
    index: usize,
}

impl Decoder {
    pub fn new(options: Options) -> Result<Self> {
        options.validate()?;

        let window_size = 1 << options.window_bits;
        let lookahead_max = 1 << options.lookahead_bits;

        Ok(Self {
            options,
            state: State::Start,
            input_prefix: None,
            output_window: CyclicBuffer::new(window_size),
        })
    }

    /// May return BitIoError::NotEnoughBits if there aren't enough bits left to
    /// transition between states.
    fn update_inner(&mut self, strm: &mut BitReader, out: &mut OutputBuffer) -> Result<()> {
        self.state = match self.state {
            State::Start => {
                let tag = strm.read_bits_exact(1)?;

                if tag == LITERAL_TAG {
                    let v = strm.read_bits_exact(8)? as u8;
                    out.buf[out.index] = v;
                    out.index += 1;

                    self.output_window.extend_from_slice(&[v]);

                    State::Start
                } else {
                    let distance = strm.read_bits_exact(self.options.window_bits as u8)? + 1;
                    let len = strm.read_bits_exact(self.options.lookahead_bits as u8)? + 1;

                    if unlikely(distance > self.output_window.end_offset()) {
                        return Err(err_msg("Invalid back reference. Overflow start of stream."));
                    }

                    let index = self.output_window.end_offset() - distance;

                    if unlikely(index < self.output_window.start_offset()) {
                        return Err(err_msg("Invalid back reference. Overflow start of window"));
                    }

                    State::Backreference { index, len }
                }
            }
            State::Backreference { mut index, mut len } => {
                while len > 0 && out.index < out.buf.len() {
                    let v = self.output_window[index];
                    out.buf[out.index] = v;
                    out.index += 1;
                    self.output_window.extend_from_slice(&[v]);

                    index += 1;
                    len -= 1;
                }

                if len == 0 {
                    State::Start
                } else {
                    State::Backreference { index, len }
                }
            }
        };

        Ok(())
    }
}

impl Transform for Decoder {
    fn update(
        &mut self,
        input: &[u8],
        end_of_input: bool,
        output: &mut [u8],
    ) -> Result<TransformProgress> {
        // TODO: Deduplicate this with the 'Inflater' struct.

        let mut cursor = std::io::Cursor::new(&input);
        let mut strm = BitReader::new_with_order(&mut cursor, BitOrder::MSBFirst);

        if let Some(v) = self.input_prefix.take() {
            strm.load_raw(v)?;
        }

        let mut out = OutputBuffer {
            buf: output,
            index: 0,
        };

        while out.index < out.buf.len() {
            match self.update_inner(&mut strm, &mut out) {
                Ok(_) => {}
                Err(e) => {
                    if let Some(BitIoError::NotEnoughBits) = e.downcast_ref() {
                        break;
                    }

                    return Err(e);
                }
            };

            strm.consume();
        }

        // NOTE: This may be non-empty at the end if the used bits don't evenly end on a
        // byte offset.
        self.input_prefix = Some(strm.into_unconsumed_raw());

        let input_read = cursor.position() as usize;
        let output_written = out.index;
        let done = input_read == input.len() && end_of_input && self.state == State::Start;

        Ok(TransformProgress {
            input_read,
            output_written,
            done,
            event: (),
        })
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    use compression::transform::transform_to_vec;

    #[test]
    fn decoder_standard_tests() {
        // Test cases from the standard library: https://github.com/atomicobject/heatshrink/blob/master/test_heatshrink_dynamic.c
        let decode_tests: &'static [(usize, usize, &'static [u8], &'static [u8])] = &[
            (7, 6, &[0xb3, 0x5b, 0xed, 0xe0, 0x41, 0x00], b"foofoo"),
            (8, 7, &[0xb0, 0x80, 0x01, 0x80], b"aaaaa"),
            (7, 3, &[0xb3, 0x5b, 0xed, 0xe0], b"foo"),
            (
                8,
                3,
                &[0xb0, 0xd8, 0xac, 0x76, 0x40, 0x1b, 0xb2, 0x80],
                b"abcdabcde",
            ),
            (8, 3, &[0xb0, 0xd8, 0xac, 0x76, 0x40, 0x1b], b"abcdabcd"),
            (
                8,
                7,
                &[0x80, 0x40, 0x60, 0x50, 0x38, 0x20],
                &[0, 1, 2, 3, 4],
            ),
        ];

        for (window_bits, lookahead_bits, compressed, uncompressed) in decode_tests.iter().cloned()
        {
            let mut output = vec![];
            transform_to_vec(
                Decoder::new(Options {
                    window_bits,
                    lookahead_bits,
                })
                .unwrap(),
                compressed,
                &mut output,
            )
            .unwrap();

            assert_eq!(&output[..], uncompressed);
        }

        // Testing byte by byte decoding
        for (window_bits, lookahead_bits, compressed, uncompressed) in decode_tests.iter().cloned()
        {
            let mut output = vec![0u8; 100];

            let mut decoder = Decoder::new(Options {
                window_bits,
                lookahead_bits,
            })
            .unwrap();

            let mut out_i = 0;
            for i in 0..compressed.len() {
                let p = decoder
                    .update(&compressed[i..(i + 1)], false, &mut output[out_i..])
                    .unwrap();
                assert!(!p.done);
                assert_eq!(p.input_read, 1);
                out_i += p.output_written;
            }

            let p = decoder.update(&[], true, &mut output[out_i..]).unwrap();
            out_i += p.output_written;
            assert!(p.done);

            assert_eq!(&output[..out_i], uncompressed);
        }

        // Output buffer is the exact length it should be.
        for (window_bits, lookahead_bits, compressed, uncompressed) in decode_tests.iter().cloned()
        {
            let mut output = vec![0u8; uncompressed.len()];

            let mut decoder = Decoder::new(Options {
                window_bits,
                lookahead_bits,
            })
            .unwrap();

            let p = decoder.update(compressed, true, &mut output[..]).unwrap();

            assert!(p.done);
            assert_eq!(p.input_read, compressed.len());
            assert_eq!(p.output_written, uncompressed.len());
            assert_eq!(&output, uncompressed);
        }
    }
}
