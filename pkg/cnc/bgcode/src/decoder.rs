use base_error::*;
use compression::transform::IdentityTransform;
use compression::transform::Transform;
use compression::transform::TransformProgress;
use crypto::checksum::crc::CRC32Hasher;

use crate::decoder_raw::*;
use crate::params::*;
use crate::proto::GCodeEncodingType;
use crate::proto::{BlockHeader, BlockType, FileHeader, ThumbnailParams};

/// Max size of a single non-gcode block.
/// This limit is needed since we only split up gcode blocks when feeding back
/// to the caller.
///
/// TODO: Implement this.
const MAX_BLOCK_SIZE: usize = 128 * 1024;

#[derive(Debug)]
pub enum Event {
    Pending,

    Metadata {
        typ: BlockType,
        data: Vec<u8>,
    },
    Thumbnail {
        params: ThumbnailParams,
        data: Vec<u8>,
    },

    GCode,

    /// Emitted with no output data once all gcode data in a block has been
    /// emitted and the block checksum was verified.
    GCodeEnd,
}

pub struct Decoder {
    raw_decoder: RawDecoder,

    state: State,
}

enum State {
    NotInBlock,

    /// We are buffering the entire contents of the current block and will
    /// return the whole block to the caller once it is fully received.
    ///
    /// This state is used for thumbnail/metadata blocks (everything but gcode
    /// blocks).
    BufferingBlock {
        buffer: Vec<u8>,
        offset: usize,
        block_params: BlockParams,
    },

    GCodeBlock {
        transform: Box<dyn Transform + Send>,

        /// Intermediate buffer. Contains the uncompressed block bytes prior to
        /// going through the meat unpacker
        ///
        /// TODO: Re-use this buffer across stages.
        buffer: Vec<u8>,

        buffer_offset: usize,

        /// If true, we received all the done for this block.
        done: bool,
    },
}

impl Decoder {
    pub fn new() -> Self {
        Self {
            raw_decoder: RawDecoder::new(),
            state: State::NotInBlock,
        }
    }

    // TODO: Need to have a clearer signal as to when the updates are fully done.
    pub fn update(
        &mut self,
        input: &[u8],
        end_of_input: bool,
        output: &mut [u8],
    ) -> Result<TransformProgress<Event>> {
        let mut input_read = 0;
        let mut output_written = 0;

        // TODO: We should be allowed to mark things as
        loop {
            match &mut self.state {
                State::NotInBlock => {
                    let inner_progress =
                        self.raw_decoder
                            .update(&input[input_read..], end_of_input, &mut [])?;
                    input_read += inner_progress.input_read;

                    if inner_progress.done {
                        return Ok(TransformProgress {
                            input_read,
                            output_written,
                            done: true,
                            event: Event::Pending,
                        });
                    }

                    match inner_progress.event {
                        RawEvent::Pending => {
                            // Need more input.
                            break;
                        }
                        RawEvent::BlockHeader(header, params) => match params {
                            BlockParams::Metadata(_) | BlockParams::Thumbnail(_) => {
                                self.state = State::BufferingBlock {
                                    buffer: vec![
                                        0u8;
                                        header.uncompressed_size.unwrap_or(header.compressed_size)
                                            as usize
                                    ],
                                    offset: 0,
                                    block_params: params,
                                }
                            }
                            BlockParams::GCode(v) => {
                                let transform: Box<dyn Transform + Send> = match v.encoding {
                                    GCodeEncodingType::NoEncoding => {
                                        Box::new(IdentityTransform::default())
                                    }
                                    GCodeEncodingType::Meatpack
                                    | GCodeEncodingType::MeatpackWithComments => {
                                        Box::new(meatpack::Decoder::new())
                                    }
                                    GCodeEncodingType::Unknown(v) => {
                                        return Err(format_err!(
                                            "Unsupported gcode encoding type: {}",
                                            v
                                        ))
                                    }
                                };

                                self.state = State::GCodeBlock {
                                    transform,
                                    buffer: vec![],
                                    buffer_offset: 0,
                                    done: false,
                                };
                            }
                        },
                        RawEvent::BlockData | RawEvent::BlockDone => {
                            return Err(err_msg("Unexpected block data outside of a block"));
                        }
                    }
                }
                State::BufferingBlock {
                    buffer,
                    offset,
                    block_params,
                } => {
                    let inner_progress = self.raw_decoder.update(
                        &input[input_read..],
                        end_of_input,
                        &mut buffer[*offset..],
                    )?;
                    input_read += inner_progress.input_read;
                    *offset += inner_progress.output_written;

                    match inner_progress.event {
                        RawEvent::BlockData => {}
                        RawEvent::BlockDone => {
                            let event = {
                                match block_params {
                                    BlockParams::Thumbnail(v) => Event::Thumbnail {
                                        params: v.clone(),
                                        // TODO: Avoid this clone.
                                        data: buffer.clone(),
                                    },
                                    _ => Event::Pending,
                                }
                            };

                            self.state = State::NotInBlock;

                            return Ok(TransformProgress {
                                input_read,
                                output_written,
                                done: inner_progress.done,
                                event,
                            });
                        }
                        RawEvent::Pending => {
                            return Ok(TransformProgress {
                                event: Event::Pending,
                                input_read,
                                output_written,
                                done: false,
                            })
                        }
                        _ => {
                            return Err(err_msg("Unexpected parsing event in thumbnail"));
                        }
                    }
                }
                State::GCodeBlock {
                    transform,
                    buffer,
                    buffer_offset,
                    done,
                } => {
                    // TODO: Generalize this as a nested Transform pattern.
                    let mut inner_done = false;
                    while !inner_done {
                        let progress = transform.update(
                            &buffer[*buffer_offset..],
                            *done,
                            &mut output[output_written..],
                        )?;

                        *buffer_offset += progress.input_read;
                        output_written += progress.output_written;
                        inner_done = progress.done;

                        if progress.input_read == 0 && progress.output_written == 0 {
                            break;
                        }
                    }

                    if output_written != 0 {
                        // TODO: Support 'done' here.
                        return Ok(TransformProgress {
                            input_read,
                            output_written,
                            done: false,
                            event: Event::GCode,
                        });
                    }

                    // We will always try to flush the buffer before pulling more raw bytes from the
                    // underlying block.
                    if *buffer_offset != buffer.len() {
                        // Need more input
                        break;
                    }

                    if *done {
                        if !inner_done {
                            // Need more output space to which to write.
                            break;
                        }

                        self.state = State::NotInBlock;

                        return Ok(TransformProgress {
                            input_read,
                            output_written,
                            done: false,
                            event: Event::GCodeEnd,
                        });
                    }

                    *buffer_offset = 0;
                    buffer.resize(4 * 1024, 0);
                    let progress =
                        self.raw_decoder
                            .update(&input[input_read..], end_of_input, buffer)?;
                    input_read += progress.input_read;
                    buffer.truncate(progress.output_written);

                    match progress.event {
                        RawEvent::Pending => {
                            // Need more input data.
                            break;
                        }
                        RawEvent::BlockData => {}
                        // TODO: Predict BlockDone ahead of time once we got all the data? (though
                        // this shouldn't be confused by the checksum passing).
                        RawEvent::BlockDone => {
                            *done = true;
                        }
                        _ => {
                            return Err(err_msg("Unexpected event in gcode block"));
                        }
                    }
                }
            }
        }

        Ok(TransformProgress {
            input_read,
            output_written,
            done: false,
            event: Event::Pending,
        })
    }
}
