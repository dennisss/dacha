use base_error::*;
use common::array_ref;
use compression::transform::{Transform, TransformProgress};
use crypto::checksum::crc::CRC32Hasher;
use crypto::hasher::Hasher;

use crate::params::BlockParams;
use crate::proto::{
    BlockHeader, BlockType, ChecksumType, CompressionType, FileHeader, ThumbnailParams,
};
use crate::FILE_MAGIC;

/// Minimum number of bytes we need to have before we try to parse a file|block
/// header + block parameters.
///
/// This must be larger than the biggest header + biggest block params struct.
const MIN_HEADER_BUFFER: usize = 32;

#[derive(Clone, Debug)]
pub enum RawEvent {
    /// Default event. We consumed input but we aren't able to output any data.
    ///
    /// No data is output with this event.
    Pending,

    /// No data is output with this event.
    BlockHeader(BlockHeader, BlockParams),

    /// Uncompressed block data.
    /// (the data was written into the 'output' argument given to 'update()')
    BlockData,

    /// The current block is done being read and had a valid checksum (if
    /// checksumming is enabled).
    ///
    /// No data is output with this event.
    BlockDone,
}

/// Handles decoding of a bgcode file into a stream of block chunks.
///
/// - This handles removing block level compression and doing block checksum
///   checks.
/// - Block type specific stuff like decoding gcode meatpack compression needs
///   to be handled by the caller.
pub struct RawDecoder {
    state: State,

    /// Input bytes that have been buffered because we have too few to do more
    /// parsing.
    input_buffer: InputBuffer,

    /// Header for the file. Present in every state other than 'StartOfFile'.
    file_header: Option<FileHeader>,
}

enum State {
    /// Waiting for the file header.
    StartOfFile,

    /// Waiting for a block header (+ parameters)
    StartOfBlock,

    Block {
        transform: Box<dyn Transform + Send>,

        transform_done: bool,

        hasher: Option<CRC32Hasher>,

        /// Remaining number of compressed input data bytes we need to read from
        /// the input file for this block.
        remaining_input: usize,

        /// Total number of uncompressed bytes produced for this block so far.
        total_output: usize,

        /// The total number of uncompressed bytes we expect based on the block
        /// header.
        expected_uncompressed_size: usize,
    },

    BlockChecksum {
        expected_checksum: u32,
    },
}

impl RawDecoder {
    pub fn new() -> Self {
        Self {
            state: State::StartOfFile,
            input_buffer: InputBuffer {
                data: vec![],
                offset: 0,
            },
            file_header: None,
        }
    }

    /// Performs incremental decoding given more inputs.
    ///
    /// Each call to this function will always either emit an event or parse all
    /// of the input data.
    pub fn update(
        &mut self,
        input: &[u8],
        end_of_input: bool,
        output: &mut [u8],
    ) -> Result<TransformProgress<RawEvent>> {
        let mut input_read = 0;
        let mut output_written = 0;

        loop {
            match &mut self.state {
                State::StartOfFile => {
                    input_read += self
                        .input_buffer
                        .append(&input[input_read..], MIN_HEADER_BUFFER);
                    if self.input_buffer.value().len() < MIN_HEADER_BUFFER && !end_of_input {
                        // Need more input.
                        break;
                    }

                    let (file_header, rest) = FileHeader::parse(self.input_buffer.value())?;
                    self.input_buffer.offset += self.input_buffer.value().len() - rest.len();

                    if &file_header.magic[..] != FILE_MAGIC {
                        return Err(err_msg("Bad file magic"));
                    }

                    if file_header.version != 1 {
                        return Err(err_msg("Unsupported bgcode file version"));
                    }

                    self.file_header = Some(file_header);

                    self.state = State::StartOfBlock;
                }
                State::StartOfBlock => {
                    input_read += self
                        .input_buffer
                        .append(&input[input_read..], MIN_HEADER_BUFFER);
                    if self.input_buffer.value().len() < MIN_HEADER_BUFFER && !end_of_input {
                        // Need more input.
                        break;
                    }

                    if end_of_input && self.input_buffer.value().len() == 0 {
                        return Ok(TransformProgress {
                            input_read,
                            output_written,
                            done: true,
                            event: RawEvent::Pending,
                        });
                    }

                    let buffered_input_start = self.input_buffer.offset;

                    let (block_header, rest) = BlockHeader::parse(self.input_buffer.value())?;
                    self.input_buffer.offset += self.input_buffer.value().len() - rest.len();

                    let (block_params, rest) =
                        BlockParams::parse(block_header.typ, self.input_buffer.value())?;
                    self.input_buffer.offset += self.input_buffer.value().len() - rest.len();

                    let transform: Box<dyn Transform + Send> = match block_header.compression {
                        CompressionType::Uncompressed => {
                            Box::new(compression::transform::IdentityTransform::default())
                        }
                        CompressionType::Deflate => Box::new(compression::zlib::ZlibDecoder::new()),
                        CompressionType::HeatshrinkWindow11Lookahead4 => {
                            Box::new(heatshrink::Decoder::new(heatshrink::Options {
                                window_bits: 11,
                                lookahead_bits: 4,
                            })?)
                        }
                        CompressionType::HeatshrinkWindow12Lookahead4 => {
                            Box::new(heatshrink::Decoder::new(heatshrink::Options {
                                window_bits: 12,
                                lookahead_bits: 4,
                            })?)
                        }
                        CompressionType::Unknown(v) => {
                            return Err(format_err!("Unsupported block compression type: {}", v));
                        }
                    };

                    let hasher = match self.file_header.as_ref().unwrap().checksum_type {
                        ChecksumType::NoChecksum => None,
                        ChecksumType::CRC32 => {
                            let mut hasher = crypto::checksum::crc::CRC32Hasher::new();

                            // Bootstrap the hasher with the header and params data.
                            hasher.update(
                                &self.input_buffer.data
                                    [buffered_input_start..self.input_buffer.offset],
                            );

                            Some(hasher)
                        }
                        ChecksumType::Unknown(v) => {
                            return Err(format_err!("Unsupported checksum type: {}", v));
                        }
                    };

                    self.state = State::Block {
                        transform,
                        transform_done: false,
                        hasher,
                        remaining_input: block_header.compressed_size as usize,
                        total_output: 0,
                        expected_uncompressed_size: block_header
                            .uncompressed_size
                            .unwrap_or(block_header.compressed_size)
                            as usize,
                    };

                    return Ok(TransformProgress {
                        input_read,
                        output_written,
                        done: false,
                        event: RawEvent::BlockHeader(block_header, block_params),
                    });
                }
                State::Block {
                    transform,
                    transform_done,
                    hasher,
                    remaining_input,
                    total_output,
                    expected_uncompressed_size,
                } => {
                    while *remaining_input > 0
                        && *total_output < *expected_uncompressed_size
                        && output_written < output.len()
                    {
                        if *transform_done {
                            return Err(err_msg(
                                "Inner transform done but there are still more bytes to process",
                            ));
                        }

                        let mut input_slice = {
                            if !self.input_buffer.value().is_empty() {
                                self.input_buffer.value()
                            } else {
                                &input[input_read..]
                            }
                        };

                        // Limit end of input to the end of this block.
                        let n = core::cmp::min(*remaining_input, input_slice.len());
                        input_slice = &input_slice[..n];

                        if n == 0 {
                            break;
                        }

                        let p = transform.update(
                            &input_slice,
                            n == *remaining_input,
                            &mut output[output_written..],
                        )?;

                        if let Some(h) = hasher {
                            h.update(&input_slice[0..p.input_read]);
                        }

                        if self.input_buffer.value().is_empty() {
                            input_read += p.input_read;
                        } else {
                            self.input_buffer.offset += p.input_read;
                        }
                        *remaining_input -= p.input_read;

                        output_written += p.output_written;
                        *total_output += p.output_written;

                        *transform_done = p.done;
                    }

                    if output_written != 0 {
                        return Ok(TransformProgress {
                            input_read,
                            output_written,
                            done: false,
                            event: RawEvent::BlockData,
                        });
                    }

                    let available_input_bytes =
                        self.input_buffer.value().len() + (input.len() - input_read);

                    if *remaining_input == 0 || *total_output >= *expected_uncompressed_size {
                        if *remaining_input != 0 {
                            return Err(err_msg(
                                "Block decompressed to expected length but still has extra bytes.",
                            ));
                        }

                        if !*transform_done {
                            return Err(err_msg(
                                "Consumed all block data, but not fully marked as decompressed.",
                            ));
                        }

                        if *total_output != *expected_uncompressed_size {
                            return Err(err_msg(
                                "Incorrect number of bytes decompressed from block",
                            ));
                        }

                        if let Some(hasher) = hasher {
                            let expected_checksum = hasher.finish_u32();
                            self.state = State::BlockChecksum { expected_checksum };
                            continue;
                        } else {
                            self.state = State::StartOfBlock;

                            return Ok(TransformProgress {
                                input_read,
                                output_written,
                                done: end_of_input && available_input_bytes == 0,
                                event: RawEvent::BlockDone,
                            });
                        }
                    }

                    if end_of_input {
                        if available_input_bytes < *remaining_input {
                            return Err(err_msg(
                                "At end of inputs, but not enough data to complete block",
                            ));
                        }
                    }

                    break;
                }
                State::BlockChecksum { expected_checksum } => {
                    input_read += self.input_buffer.append(&input[input_read..], 4);

                    if self.input_buffer.value().len() < 4 {
                        if end_of_input {
                            return Err(err_msg("Missing checksum at end of block"));
                        }

                        // Retry once we have more input.
                        break;
                    }

                    let checksum = u32::from_le_bytes(*array_ref![self.input_buffer.value(), 0, 4]);
                    self.input_buffer.offset += 4;

                    if checksum != *expected_checksum {
                        return Err(format_err!(
                            "Incorrect block checksum: {:2x} : {:2x}",
                            checksum,
                            expected_checksum
                        ));
                    }

                    self.state = State::StartOfBlock;

                    return Ok(TransformProgress {
                        input_read,
                        output_written,
                        done: false, // TODO
                        event: RawEvent::BlockDone,
                    });
                }
            }
        }

        Ok(TransformProgress {
            input_read,
            output_written,
            done: false, // TODO
            event: RawEvent::Pending,
        })
    }
}

struct InputBuffer {
    data: Vec<u8>,
    offset: usize,
}

impl InputBuffer {
    fn value(&self) -> &[u8] {
        &self.data[self.offset..]
    }

    fn append(&mut self, input: &[u8], max_count: usize) -> usize {
        if self.offset == self.data.len() {
            self.offset = 0;
            self.data.clear();
        }

        if self.value().len() >= max_count {
            return 0;
        }

        let n = core::cmp::min(max_count - self.value().len(), input.len());
        self.data.extend_from_slice(&input[0..n]);

        n
    }
}
