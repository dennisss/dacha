// Implementation of ZLIB compressed data format as described in https://www.ietf.org/rfc/rfc1950.txt
// No relation to the zlib C library.

// Big endian integers

use std::convert::{TryFrom, TryInto};
use std::io::{Read, Write};

use byteorder::{BigEndian, ReadBytesExt, WriteBytesExt};
use common::errors::*;
use crypto::checksum::adler32::*;
use crypto::hasher::*;

use crate::deflate::*;
use crate::buffer_queue::BufferQueue;
use crate::transform::{Transform, TransformProgress};

const WINDOW_LOG_OFFSET: u8 = 8;

struct DeflateInfo {
    /// LZ77 window size used by the compressor in bytes.
    window_size: usize,
}

enum CompressionMethod {
    Deflate(DeflateInfo),
}

impl CompressionMethod {
    fn decode(cmf: u8) -> Result<CompressionMethod> {
        let cm = cmf & 0b1111;
        let cinfo = cmf >> 4;

        Ok(match cm {
            8 => {
                let size = 1 << (cinfo + WINDOW_LOG_OFFSET) as usize;
                if size > 32768 {
                    return Err(err_msg("Window size too large for deflate"));
                }

                CompressionMethod::Deflate(DeflateInfo { window_size: size })
            }
            _ => {
                return Err(err_msg("Unknown compression method"));
            }
        })
    }
}

enum CompressionLevel {
    Fastest = 0,
    Fast = 1,
    Default = 2,
    /// Maximum compression
    Slowest = 3,
}

impl TryFrom<u8> for CompressionLevel {
    type Error = Error;
    fn try_from(v: u8) -> Result<CompressionLevel> {
        Ok(match v {
            0 => CompressionLevel::Fastest,
            1 => CompressionLevel::Fast,
            2 => CompressionLevel::Default,
            3 => CompressionLevel::Slowest,
            // NOTE: Will never happen as we will always use a 4 bit integer.
            _ => {
                return Err(err_msg("Invalid compression level"));
            }
        })
    }
}

struct Header {
    compression_method: CompressionMethod,
    compression_level: CompressionLevel,
    // Adler32 of the dictionary being used.
    dictid: Option<u32>,
}

struct Trailer {
    uncompressed_checksum: u32
}


pub struct ZlibDecoder {
    input_buffer: BufferQueue,
    state: ZlibDecoderState,
    header: Option<Header>,
    hasher: Adler32Hasher,
    inflater: Inflater
}

impl ZlibDecoder {
    pub fn new() -> Self {
        Self {
            input_buffer: BufferQueue::new(),
            state: ZlibDecoderState::Header,
            header: None,
            hasher: Adler32Hasher::new(),
            inflater: Inflater::new()
        }

    }

    fn update_impl(
        &mut self,
        mut input: &[u8],
        end_of_input: bool,
        mut output: &mut [u8],        
    ) -> Result<TransformProgress> {
        let mut input_read = 0;
        let mut output_written = 0;
        let mut done = false;

        loop {
            match self.state.clone() {
                ZlibDecoderState::Header => {
                    let (maybe_header, n) = self.input_buffer.try_read(input, Self::read_header)?;
                    input = &input[n..];
                    input_read += n;

                    if let Some(header) = maybe_header {
                        // TODO: Check actually using deflate

                        self.state = ZlibDecoderState::Body;
                    } else {
                        // Need more bytes to read the header.
                        break;
                    }
                }
                ZlibDecoderState::Body => {
                    // TODO: Implement dictionary and pass in window size.
                    let inner_progress = self.inflater.update(input, end_of_input, output)?;
                    
                    input_read += inner_progress.input_read;
                    input = &input[inner_progress.input_read..];

                    output_written += inner_progress.output_written;
                    self.hasher.update(&output[0..inner_progress.output_written]);

                    if inner_progress.done {
                        self.state = ZlibDecoderState::Trailer;
                    } else {
                        // Won't make any progress without more inputs/outputs.
                        break;
                    }
                }
                ZlibDecoderState::Trailer => {
                    let (maybe_trailer, n) = self.input_buffer.try_read(input, Self::read_trailer)?;
                    input = &input[n..];
                    input_read += n;

                    if let Some(trailer) = maybe_trailer {
                        let actual_checksum = self.hasher.finish_u32();

                        if trailer.uncompressed_checksum != actual_checksum {
                            return Err(err_msg("Invalid checksum"));
                        }

                        done = true;
                        self.state = ZlibDecoderState::Done;
                    }

                    break;
                }
                ZlibDecoderState::Done => {
                    return Err(err_msg("ZlibDecoder already done"));
                }

            }
        }
        
        Ok(TransformProgress {
            input_read,
            output_written,
            done
        })
    }

    fn read_header(reader: &mut dyn Read) -> Result<Header> {
        let mut header = [0u8; 2];
        reader.read_exact(&mut header)?;
    
        let cmf = header[0];
        let flg = header[1];
        if ((cmf as usize) * 256 + (flg as usize)) % 31 != 0 {
            return Err(err_msg("Invalid header bytes"));
        }
    
        let compression_method = CompressionMethod::decode(cmf)?;
        let fcheck = flg & 0b1111; // Checked above.
        let fdict = (flg >> 5) & 0b1;
        let flevel = (flg >> 6) & 0b11;
    
        let dictid = if fdict == 1 {
            Some(reader.read_u32::<BigEndian>()?)
        } else {
            None
        };

        Ok(Header {
            compression_method,
            compression_level: CompressionLevel::try_from(flevel)?,
            dictid,
        })
    }

    fn read_trailer(reader: &mut dyn Read) -> Result<Trailer> {
        let uncompressed_checksum = reader.read_u32::<BigEndian>()?;
        Ok(Trailer { uncompressed_checksum })
    }
}

impl Transform for ZlibDecoder {
    fn update(
        &mut self,
        input: &[u8],
        end_of_input: bool,
        output: &mut [u8],        
    ) -> Result<TransformProgress> {
        self.update_impl(input, end_of_input, output)
    }
}

#[derive(Clone)]
enum ZlibDecoderState {
    Header,
    Body,
    Trailer,
    Done
}

// TODO: Implement Write path
