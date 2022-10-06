// Implementation of ZLIB compressed data format as described in https://www.ietf.org/rfc/rfc1950.txt
// No relation to the zlib C library.

// Big endian integers

use std::convert::{TryFrom, TryInto};
use std::io::{Read, Write};

use byteorder::{BigEndian, ReadBytesExt, WriteBytesExt};
use common::errors::*;
use crypto::checksum::adler32::*;
use crypto::hasher::*;

use crate::buffer_queue::BufferQueue;
use crate::deflate::*;
use crate::transform::{Transform, TransformProgress};

const WINDOW_LOG_OFFSET: u8 = 8;

struct DeflateInfo {
    /// LZ77 window size used by the compressor in bytes.
    window_size: usize,
}

enum CompressionMethod {
    Deflate(DeflateInfo),
}

impl Default for CompressionMethod {
    fn default() -> Self {
        CompressionMethod::Deflate(DeflateInfo { window_size: 32768 })
    }
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

    fn encode(&self) -> Result<u8> {
        Ok(match self {
            CompressionMethod::Deflate(info) => {
                if info.window_size > 32768 {
                    return Err(err_msg("Window size too large for deflate"));
                }

                let window_log = info.window_size.ilog2();
                if window_log < WINDOW_LOG_OFFSET as u32 {
                    return Err(err_msg("Window size too small"));
                }

                if info.window_size != (1 << window_log) {
                    return Err(err_msg("Window size not a power of 2"));
                }

                ((window_log - WINDOW_LOG_OFFSET as u32) as u8) << 4 | 8
            }
        })
    }
}

#[derive(Clone, Copy)]
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

impl Header {
    fn read(reader: &mut dyn Read) -> Result<Header> {
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

    fn serialize(&self, out: &mut Vec<u8>) -> Result<()> {
        let cmf = self.compression_method.encode()?;

        let mut flg = 0;
        if self.dictid.is_some() {
            flg |= 1 << 5;
        }
        flg |= (self.compression_level as u8) << 6;

        let fcheck = 31 - (((cmf as usize) * 256 + (flg as usize)) % 31);
        flg |= fcheck as u8;

        out.push(cmf);
        out.push(flg);
        if let Some(dictid) = self.dictid {
            out.extend_from_slice(&dictid.to_be_bytes());
        }

        Ok(())
    }
}

struct Trailer {
    uncompressed_checksum: u32,
}

pub struct ZlibEncoder {
    // TODO: This will be up to 6 bytes.
    pending_bytes: Vec<u8>,

    hasher: Adler32Hasher,
    deflater: Deflater,

    deflater_done: bool,
}

impl ZlibEncoder {
    pub fn new() -> Self {
        let header = Header {
            compression_method: CompressionMethod::default(),
            compression_level: CompressionLevel::Default,
            dictid: None,
        };

        let mut pending_bytes = vec![];
        header.serialize(&mut pending_bytes).unwrap();

        let hasher = Adler32Hasher::new();

        Self {
            deflater: Deflater::new(),
            hasher,
            pending_bytes,
            deflater_done: false,
        }
    }
}

impl Transform for ZlibEncoder {
    fn update(
        &mut self,
        mut input: &[u8],
        end_of_input: bool,
        mut output: &mut [u8],
    ) -> Result<TransformProgress> {
        let mut input_read = 0;
        let mut output_written = 0;

        loop {
            if !self.pending_bytes.is_empty() {
                let n = std::cmp::min(self.pending_bytes.len(), output.len());
                output[0..n].copy_from_slice(&self.pending_bytes[0..n]);
                output = &mut output[n..];
                output_written += n;
                self.pending_bytes = self.pending_bytes[n..].to_vec();

                if !self.pending_bytes.is_empty() {
                    break;
                }
            }

            if !self.deflater_done {
                let progress = self.deflater.update(input, end_of_input, output)?;

                self.hasher.update(&input[0..progress.input_read]);

                input_read += progress.input_read;
                input = &input[progress.input_read..];

                output_written += progress.output_written;
                output = &mut output[progress.output_written..];

                self.deflater_done = progress.done;

                if self.deflater_done {
                    self.pending_bytes
                        .extend_from_slice(&self.hasher.finish_u32().to_be_bytes());
                    continue;
                }
            }

            break;
        }

        Ok(TransformProgress {
            input_read,
            output_written,
            done: self.pending_bytes.is_empty() && self.deflater_done,
        })
    }
}

pub struct ZlibDecoder {
    input_buffer: BufferQueue,
    state: ZlibDecoderState,
    header: Option<Header>,
    hasher: Adler32Hasher,
    inflater: Inflater,
}

impl ZlibDecoder {
    pub fn new() -> Self {
        Self {
            input_buffer: BufferQueue::new(),
            state: ZlibDecoderState::Header,
            header: None,
            hasher: Adler32Hasher::new(),
            inflater: Inflater::new(),
        }
    }

    fn update_impl(
        &mut self,
        mut input: &[u8],
        end_of_input: bool,
        output: &mut [u8],
    ) -> Result<TransformProgress> {
        let mut input_read = 0;
        let mut output_written = 0;
        let mut done = false;

        loop {
            match self.state.clone() {
                ZlibDecoderState::Header => {
                    let (maybe_header, n) = self.input_buffer.try_read(input, Header::read)?;
                    input = &input[n..];
                    input_read += n;

                    if let Some(header) = maybe_header {
                        match header.compression_method {
                            CompressionMethod::Deflate(_) => {}
                        }

                        if header.dictid.is_some() {
                            return Err(err_msg("Dictionaries not supported"));
                        }

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
                    self.hasher
                        .update(&output[0..inner_progress.output_written]);

                    if inner_progress.done {
                        self.state = ZlibDecoderState::Trailer;
                    } else {
                        // Won't make any progress without more inputs/outputs.
                        break;
                    }
                }
                ZlibDecoderState::Trailer => {
                    let (maybe_trailer, n) =
                        self.input_buffer.try_read(input, Self::read_trailer)?;
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
            done,
        })
    }

    fn read_trailer(reader: &mut dyn Read) -> Result<Trailer> {
        let uncompressed_checksum = reader.read_u32::<BigEndian>()?;
        Ok(Trailer {
            uncompressed_checksum,
        })
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
    Done,
}

// TODO: Implement Write path
