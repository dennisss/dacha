use std::io::SeekFrom;

use common::async_std::fs::File;
use common::async_std::io::prelude::*;
use common::errors::*;
use compression::transform::transform_to_vec;
use compression::zlib::ZlibDecoder;
use crypto::checksum::crc::CRC32CHasher;
use crypto::hasher::Hasher;

use crate::table::block_handle::BlockHandle;
use crate::table::footer::*;

/*
 * Block format:
 * - Block contents
 * - Trailer:
 * 	- [0]: compression_type u8
 * 	- [1]: Checksum of [block_contents | compression_type]
 * 	- Padding (if a data block)
 * 		- (RocksDB will pad to a block size of 4096 by default)
 */

/// Always 1 byte for CompressionType + 4 bytes for checksum.
const BLOCK_TRAILER_SIZE: usize = 5;

#[derive(Debug)]
pub struct RawBlock {
    data: Vec<u8>,
    compression_type: CompressionType, // is_data_block
}

enum_def!(CompressionType u8 =>
    // These are supported in LevelDB or RocksDB
    None = 0,
    Snappy = 1,

    // Only supported in RocksDB
    ZLib = 2,
    BZip2 = 3,
    LZ4 = 4,
    LZ4HC = 5,
    XPress = 6,
    Zstd = 7
);

impl RawBlock {
    pub async fn read(file: &mut File, footer: &Footer, handle: &BlockHandle) -> Result<Self> {
        let mut buf = vec![];
        file.seek(SeekFrom::Start(handle.offset)).await?;
        buf.resize((handle.size as usize) + BLOCK_TRAILER_SIZE, 0);
        file.read_exact(&mut buf).await?;

        // min_size!(buf, BLOCK_TRAILER_SIZE);
        let trailer_start = buf.len() - BLOCK_TRAILER_SIZE;
        let trailer = &buf[trailer_start..];

        let compression_type = CompressionType::from_value(trailer[0])?;
        let checksum = u32::from_le_bytes(*array_ref![trailer, 1, 4]);

        let expected_checksum = match footer.checksum_type {
            ChecksumType::None => 0,
            ChecksumType::CRC32C => {
                let mut hasher = CRC32CHasher::new();
                hasher.update(&buf[..(trailer_start + 1)]);
                hasher.masked()
            }
            _ => {
                return Err(err_msg("Unsupported checksum type"));
            }
        };

        if checksum != expected_checksum {
            return Err(err_msg("Incorrect checksum in raw block"));
        }

        buf.truncate(trailer_start);

        Ok(Self {
            data: buf,
            compression_type,
        })
    }

    pub fn decompress(self) -> Result<Vec<u8>> {
        Ok(match self.compression_type {
            CompressionType::None => self.data,
            CompressionType::Snappy => {
                let mut out = vec![];
                compression::snappy::snappy_decompress(&self.data, &mut out)?;
                out
            }
            CompressionType::ZLib => {
                let mut out = vec![];
                let mut decoder = ZlibDecoder::new();
                let progress = transform_to_vec(&mut decoder, &self.data, true, &mut out)?;
                if !progress.done || progress.input_read != self.data.len() {
                    return Err(err_msg("Failed to decode full block"));
                }

                out
            }
            _ => {
                return Err(format_err!(
                    "Unsupported compression type {:?}",
                    self.compression_type
                ));
            }
        })
    }
}
