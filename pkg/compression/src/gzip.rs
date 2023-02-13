use byteorder::{ByteOrder, LittleEndian, ReadBytesExt, WriteBytesExt};
use std::convert::{TryFrom, TryInto};
use std::io::{Read, Seek, Write};

use common::bits::{bitget, bitset};
use common::errors::*;
use crypto::checksum::crc::*;
use crypto::hasher::*;
use parsing::iso::*;

use crate::buffer_queue::BufferQueue;
use crate::deflate::*;
use crate::transform::*;

// ZLib RFC http://www.zlib.org/rfc-gzip.html
// This is based on v4.3

// Also http://www.onicos.com/staff/iz/formats/gzip.html

// ISO 8859-1 (LATIN-1) strings
//

// TODO: See https://github.com/Distrotech/gzip/blob/94cfaabe3ae7640b8c0334283df37cbdd7f7a0a9/gzip.h#L161 for new and old magic bytes

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum CompressionMethod {
    // Stored = 0,
    // Compressed = 1,
    // Packed = 2,
    // LZH = 3,
    // 4..7 reserved
    Reserved = 0, // 0..7
    Deflate = 8,
}

impl std::convert::TryFrom<u8> for CompressionMethod {
    type Error = common::errors::Error;
    fn try_from(i: u8) -> Result<Self> {
        Ok(match i {
            0 | 1 | 2 | 3 | 4 | 5 | 6 | 7 => CompressionMethod::Reserved,
            8 => CompressionMethod::Deflate,
            _ => {
                return Err(err_msg("Unknown compression method"));
            }
        })
    }
}

#[derive(Debug)]
pub struct Flags {
    // Whether the text is probably ASCII.
    ftext: bool,
    // CRC16 of header is present.
    fhcrc: bool,
    // Extra field is present.
    fextra: bool,
    // Filename is present
    fname: bool,
    // Comment is present.
    fcomment: bool,
}

impl Flags {
    pub fn from_byte(i: u8) -> Flags {
        Flags {
            ftext: bitget(i, 0),
            fhcrc: bitget(i, 1),
            fextra: bitget(i, 2),
            fname: bitget(i, 3),
            fcomment: bitget(i, 4),
        }
    }

    pub fn to_byte(&self) -> u8 {
        let mut i = 0;
        bitset(&mut i, self.ftext, 0);
        bitset(&mut i, self.fhcrc, 1);
        bitset(&mut i, self.fextra, 2);
        bitset(&mut i, self.fname, 3);
        bitset(&mut i, self.fcomment, 4);
        i
    }
}

#[derive(Debug)]
pub struct Header {
    pub compression_method: CompressionMethod,

    // Whether the text is probably ASCII.
    pub is_text: bool,

    /// Seconds since unix epoch.
    pub mtime: u32,
    // TODO: Check what is supposed to be in here.
    pub extra_flags: u8,
    pub os: u8,
    //
    pub extra_field: Option<Vec<u8>>,
    pub filename: Option<String>,
    pub comment: Option<String>,
    // Whether or not a checksum was present for the header (to validate all of the above fields).
    // NOTE: If the checksum failed, then this struct would be have been emited
    pub header_validated: bool,
}

impl Header {
    fn serialize(&self, output: &mut Vec<u8>) -> Result<()> {
        let flags = Flags {
            ftext: false,
            fhcrc: false,
            fextra: self.extra_field.is_some(),
            fname: self.filename.is_some(),
            fcomment: self.comment.is_some(),
        };

        let writer = output as &mut dyn Write;

        let mut header_buf = [0u8; HEADER_SIZE];
        header_buf[0..2].copy_from_slice(GZIP_MAGIC);
        header_buf[2] = self.compression_method as u8;
        header_buf[3] = flags.to_byte();
        LittleEndian::write_u32(&mut header_buf[4..8], self.mtime);
        header_buf[8] = self.extra_flags;
        header_buf[9] = self.os;
        writer.write_all(&header_buf)?;

        if let Some(data) = &self.extra_field {
            writer.write_u32::<LittleEndian>(data.len() as u32)?;
            writer.write_all(&data)?;
        }

        let null = [0u8; 1];
        if let Some(s) = &self.filename {
            // TODO: Validate is Latin1String.
            writer.write_all(s.as_bytes())?;
            writer.write_all(&null)?;
        }

        if let Some(s) = &self.comment {
            // TODO: Validate is Latin1String.
            writer.write_all(s.as_bytes())?;
            writer.write_all(&null)?;
        }

        Ok(())
    }
}

struct Trailer {
    body_checksum: u32,
    uncompressed_size: u32,
}

#[derive(Debug)]
pub struct GZipFile {
    pub header: Header,

    pub data: Vec<u8>, /*
                       /// File byte offsets
                       pub compressed_range: (u64, u64),

                       /// CRC32 of above compressed data range.
                       pub compressed_checksum: u32,

                       /// Uncompressed size (mod 2^32) of the original input to the compressor.
                       pub input_size:  u32
                       */
}

// TODO: Set a save max limit on length.
fn read_null_terminated(reader: &mut dyn Read) -> Result<Vec<u8>> {
    let mut out = vec![];
    let mut buf = [0u8; 1];

    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            return Err(err_msg("Hit end of file before seeing null terminator"));
        }

        if buf[0] == 0 {
            break;
        } else {
            out.push(buf[0]);
        }
    }

    Ok(out)
}

const HEADER_SIZE: usize = 10;

pub const GZIP_UNIX_OS: u8 = 0x03;

const GZIP_MAGIC: &'static [u8] = &[0x1f, 0x8b];

// Most trivial to do this with a buffer because then we can perform buffering

// Reader will buffer all input until

/*
    Reader wrapper:
    - Implements Read
    - As it reads, it internally caches all read data
    - Internal cache is discarded on called
*/

/// A reader which can be at any time 'rolled' back such it
trait RecordedRead: Read {
    /// Should be called to indicate that all internal cache
    fn consume(&mut self);

    fn take(&mut self) -> Vec<u8>;
}

// struct SliceReader {

// }

// impl RecordedRead for std::io::Cursor<[u8]> {

// }

pub struct GzipDecoder {
    state: GzipDecoderState,

    input_buffer: BufferQueue,

    header: Option<Header>,

    output_size: usize,

    inflater: Inflater,

    hasher: CRC32Hasher,
    /* header: Option<Header>,
     * body_read: bool,
     * trailer_read: bool, */
}

impl GzipDecoder {
    pub fn new() -> Self {
        Self {
            state: GzipDecoderState::Header,
            input_buffer: BufferQueue::new(),
            header: None,
            output_size: 0,
            inflater: Inflater::new(),
            hasher: CRC32Hasher::new(),
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
                GzipDecoderState::Header => {
                    let (maybe_header, n) = self.input_buffer.try_read(input, Self::read_header)?;
                    input_read += n;
                    input = &input[n..];

                    if let Some(header) = maybe_header {
                        if header.compression_method != CompressionMethod::Deflate {
                            return Err(err_msg("Unsupported compression method"));
                        }

                        self.state = GzipDecoderState::Body;
                    } else {
                        break;
                    }
                }
                GzipDecoderState::Body => {
                    let inner_progress = self.inflater.update(input, end_of_input, output)?;

                    input_read += inner_progress.input_read;
                    input = &input[inner_progress.input_read..];

                    output_written += inner_progress.output_written;
                    self.hasher
                        .update(&output[0..inner_progress.output_written]);
                    self.output_size += inner_progress.output_written;

                    if inner_progress.done {
                        self.state = GzipDecoderState::Trailer;
                    } else {
                        // Won't make any progress without more inputs/outputs.
                        break;
                    }
                }
                GzipDecoderState::Trailer => {
                    let (maybe_trailer, n) =
                        self.input_buffer.try_read(input, Self::read_trailer)?;
                    input_read += n;
                    input = &input[n..];

                    if let Some(trailer) = maybe_trailer {
                        if trailer.uncompressed_size as usize != self.output_size {
                            return Err(format_err!(
                                "Footer length mismatch, expected: {}, actual: {}",
                                trailer.uncompressed_size,
                                self.output_size
                            ));
                        }

                        let actual_checksum = self.hasher.finish_u32();
                        if trailer.body_checksum != actual_checksum {
                            return Err(format_err!(
                                "Trailer wrong checksum: {:x} {:x}",
                                actual_checksum,
                                trailer.body_checksum
                            ));
                        }

                        self.state = GzipDecoderState::Done;

                        done = true;

                        // TODO: Can now clear the self.input_buffer completely.
                    }

                    break;
                }
                GzipDecoderState::Done => {
                    return Err(err_msg("GzipDecoder already done"));
                }
            }
        }

        // TODO: If we are the end of all inputs and we don't finish, return an error?

        Ok(TransformProgress {
            done,
            input_read,
            output_written,
        })
    }

    // Issue with Read is that it uses an io::Error which doesn't work very well.
    fn read_header(reader: &mut dyn Read) -> Result<Header> {
        let mut header_reader = HashReader::new(reader, CRC32Hasher::new());

        let mut header_buf = [0u8; HEADER_SIZE];
        header_reader.read_exact(&mut header_buf)?;

        if &header_buf[0..2] != GZIP_MAGIC {
            return Err(err_msg("Invalid header bytes"));
        }

        let compression_method = CompressionMethod::try_from(header_buf[2])?;

        let flags = Flags::from_byte(header_buf[3]);

        let mtime = LittleEndian::read_u32(&header_buf[4..8]);

        let extra_flags = header_buf[8];
        let os = header_buf[9];

        let extra_field = if flags.fextra {
            let xlen = header_reader.read_u32::<LittleEndian>()? as usize;
            let mut field = vec![];
            field.resize(xlen, 0);
            header_reader.read_exact(&mut field)?;
            Some(field)
        } else {
            None
        };

        let filename = if flags.fname {
            let data = read_null_terminated(&mut header_reader)?;
            Some(Latin1String::from_bytes(data.into())?.to_string())
        } else {
            None
        };

        let comment = if flags.fcomment {
            let data = read_null_terminated(&mut header_reader)?;
            Some(Latin1String::from_bytes(data.into())?.to_string())
        } else {
            None
        };

        let header_sum = header_reader.into_hasher().finish_u32();

        let header_validated = if flags.fhcrc {
            let stored_checksum = reader.read_u16::<LittleEndian>()?;
            println!("{:x} {:x}", header_sum, stored_checksum);

            // TODO: Compare it

            true
        } else {
            false
        };

        Ok(Header {
            compression_method,
            is_text: flags.ftext,
            mtime,
            extra_flags,
            os,
            extra_field,
            filename,
            comment,
            header_validated,
        })
    }

    fn read_trailer(reader: &mut dyn Read) -> Result<Trailer> {
        let body_checksum = reader.read_u32::<LittleEndian>()?;
        let uncompressed_size = reader.read_u32::<LittleEndian>()?;

        Ok(Trailer {
            body_checksum,
            uncompressed_size,
        })
    }
}

impl Transform for GzipDecoder {
    fn update(
        &mut self,
        input: &[u8],
        end_of_input: bool,
        output: &mut [u8],
    ) -> Result<TransformProgress> {
        self.update_impl(input, end_of_input, output)
    }
}

#[derive(Clone, Debug)]
enum GzipDecoderState {
    /// Very start of the file including all conditional fields
    Header,

    /// This will need to have an Inflater, and a rolling checksum calculator
    /// (for either )
    Body,

    Trailer,

    Done,
}

// TODO: Must operate on the uncompressed data.
fn is_text(data: &[u8]) -> bool {
    for b in data.iter().cloned() {
        if b == 9 || b == 10 || b == 13 || (b >= 32 && b <= 126) {
            // Good. Keep going.
        } else {
            return false;
        }
    }

    true
}

pub struct GzipEncoder {
    output_buffer: BufferQueue,

    deflater: Deflater,

    hasher: CRC32Hasher,

    /// Total size of all uncompressed input data seen so far.
    input_size: usize,

    trailer_written: bool,
}

impl GzipEncoder {
    /// Creates a new encoder which will write minimal gzip metadata (no
    /// filename, mtime, etc.).
    pub fn default_without_metadata() -> Self {
        let header = Header {
            compression_method: CompressionMethod::Deflate,
            is_text: false,
            mtime: 0,
            extra_flags: 2, // < Max compression (slowest algorithm)
            os: GZIP_UNIX_OS,
            extra_field: None,
            filename: None,
            comment: None,
            header_validated: false,
        };

        Self::new(header).unwrap()
    }

    pub fn new(header: Header) -> Result<Self> {
        let mut output_buffer = BufferQueue::new();
        header.serialize(&mut output_buffer.buffer);

        if header.compression_method != CompressionMethod::Deflate {
            return Err(err_msg("Only deflate"));
        }

        Ok(Self {
            output_buffer,
            deflater: Deflater::new(),
            hasher: CRC32Hasher::new(),
            input_size: 0,
            trailer_written: false,
        })
    }

    pub fn update_impl(
        &mut self,
        input: &[u8],
        end_of_input: bool,
        mut output: &mut [u8],
    ) -> Result<TransformProgress> {
        let mut input_read = 0;
        let mut output_written = 0;

        // Copy any pending output to the provided buffer.
        {
            let n = self.output_buffer.copy_to(output);
            output_written += n;
            output = &mut output[n..];
        }

        // Run compression
        // NOTE: The inner compressor will never push bytes into the user's output
        // buffer until self.output_buffer is empty.
        let inner_progress = self.deflater.update(input, end_of_input, output)?;

        // Advance input counters and hasher.
        input_read += inner_progress.input_read;
        self.input_size += inner_progress.input_read;
        self.hasher.update(&input[0..input_read]);

        // Advance output.
        output_written += inner_progress.output_written;
        output = &mut output[inner_progress.output_written..];

        // Enqueue trailer to be written if all compressed bytes have been written.
        if inner_progress.done && !self.trailer_written {
            self.output_buffer
                .buffer
                .extend_from_slice(&self.hasher.finish_u32().to_le_bytes());
            self.output_buffer
                .buffer
                .extend_from_slice(&(self.input_size as u32).to_le_bytes());
            self.trailer_written = true;
        }

        // Maybe copy the trailer to the output if there is still space remaining.
        output_written += self.output_buffer.copy_to(output);

        let done = self.trailer_written && self.output_buffer.is_empty();
        Ok(TransformProgress {
            input_read,
            output_written,
            done,
        })
    }
}

impl Transform for GzipEncoder {
    fn update(
        &mut self,
        input: &[u8],
        end_of_input: bool,
        output: &mut [u8],
    ) -> Result<TransformProgress> {
        self.update_impl(input, end_of_input, output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gzip_decoder_on_testdata() -> Result<()> {
        let root_dir = std::path::Path::new("../../");

        let test_cases: &[(&'static str, &'static str)] = &[
            (
                "testdata/gutenberg/shakespeare.txt",
                "testdata/derived/shakespeare.txt.9.gz",
            ),
            (
                "testdata/gutenberg/shakespeare.txt",
                "testdata/derived/shakespeare.txt.1.gz",
            ),
            (
                "testdata/gutenberg/shakespeare.txt",
                "testdata/derived/shakespeare.txt.2.gz",
            ),
            (
                "testdata/gutenberg/shakespeare.txt",
                "testdata/derived/shakespeare.txt.4.gz",
            ),
            (
                "testdata/random/random_100",
                "testdata/derived/random_100.5.gz",
            ),
            (
                "testdata/random/random_463",
                "testdata/derived/random_463.5.gz",
            ),
            (
                "testdata/random/random_4096",
                "testdata/derived/random_4096.5.gz",
            ),
            (
                "testdata/random/random_1048576",
                "testdata/derived/random_1048576.5.gz",
            ),
        ];

        for (uncompressed_path, compressed_path) in test_cases.iter().clone() {
            println!("{:?} {:?}", uncompressed_path, compressed_path);

            let uncompressed = std::fs::read(root_dir.join(uncompressed_path))?;
            let compressed = std::fs::read(root_dir.join(compressed_path))?;

            // Decode the golden
            {
                let mut decoder = GzipDecoder::new();

                let mut uncompressed_test = vec![];
                crate::transform::transform_to_vec(
                    &mut decoder,
                    &compressed,
                    true,
                    &mut uncompressed_test,
                )?;

                assert_eq!(uncompressed_test, uncompressed);
            }

            // Encode and decode.
            {
                let mut compressed_test = vec![];
                let mut encoder = GzipEncoder::default_without_metadata();
                crate::transform::transform_to_vec(
                    &mut encoder,
                    &uncompressed,
                    true,
                    &mut compressed_test,
                )?;

                let mut decoder = GzipDecoder::new();

                let mut uncompressed_test = vec![];
                crate::transform::transform_to_vec(
                    &mut decoder,
                    &compressed_test,
                    true,
                    &mut uncompressed_test,
                )?;

                assert_eq!(uncompressed_test, uncompressed);
            }
        }

        // TODO: Need to attempt decompressing while at different byte offsets

        Ok(())
    }
}
