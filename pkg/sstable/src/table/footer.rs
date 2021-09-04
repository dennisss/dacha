use std::io::SeekFrom;

use common::async_std::{fs::File, io::prelude::SeekExt};
use common::errors::*;
use common::futures::AsyncReadExt;

use crate::encoding::check_padding;
use crate::table::block_handle::BlockHandle;

// TODO: There are two different versions:
// https://github.com/facebook/rocksdb/blob/master/table/block_based/block_based_table_builder.cc#L208
const BLOCK_BASED_MAGIC: u64 = 0x88e241b785f4cff7;
/// This is compatible with LevelDB.
const BLOCK_BASED_MAGIC_LEGACY: u64 = 0xdb4775248b80fb57;
const MAGIC_SIZE: usize = 8;

const BLOCK_HANDLE_MAX_SIZE: usize = 20;

const LEGACY_FOOTER_SIZE: usize = 2 * BLOCK_HANDLE_MAX_SIZE + MAGIC_SIZE;
const FOOTER_SIZE: usize = 2 * BLOCK_HANDLE_MAX_SIZE + 1 + 4 + MAGIC_SIZE;

// From https://github.com/facebook/rocksdb/blob/ca7ccbe2ea6be042f90f31eb75ad4dca032dbed1/table/format.cc#L163:
// legacy footer format:
//    metaindex handle (varint64 offset, varint64 size)
//    index handle     (varint64 offset, varint64 size)
//    <padding> to make the total size 2 * BlockHandle::kMaxEncodedLength
//    table_magic_number (8 bytes)
// new footer format:
//    checksum type (char, 1 byte)
//    metaindex handle (varint64 offset, varint64 size)
//    index handle     (varint64 offset, varint64 size)
//    <padding> to make the total size 2 * BlockHandle::kMaxEncodedLength + 1
//    footer version (4 bytes)
//    table_magic_number (8 bytes)
#[derive(Debug)]
pub struct Footer {
    pub checksum_type: ChecksumType,
    pub metaindex_handle: BlockHandle,
    pub index_handle: BlockHandle,

    /// Version of the format as stored in the footer. This should be the
    /// same value as the format version stored in the RocksDB properties.
    /// Version 0 is with old  RocksDB versions and LevelDB. checksum_type
    /// is only supported as non-CRC32C if footer_version >= 1
    pub footer_version: u32,
}

impl Footer {
    pub async fn read_from_file(file: &mut File) -> Result<Self> {
        let metadata = file.metadata().await?;
        let len = metadata.len();
        if len < (FOOTER_SIZE as u64) {
            return Err(err_msg("File too small"));
        }

        file.seek(SeekFrom::Start(len - (FOOTER_SIZE as u64)))
            .await?;
        let mut buf = [0u8; FOOTER_SIZE];
        file.read_exact(&mut buf).await?;
        Footer::parse(&buf)
    }

    /// Parses a footer from the given buffer. This assumes that the input is
    /// contains at least the entire footer but should not contain any data
    /// after the footer.
    pub fn parse(mut input: &[u8]) -> Result<Self> {
        min_size!(input, MAGIC_SIZE);
        let magic_start = input.len() - MAGIC_SIZE;
        let magic = u64::from_le_bytes(*array_ref![input, input.len() - MAGIC_SIZE, MAGIC_SIZE]);

        if magic == BLOCK_BASED_MAGIC {
            min_size!(input, FOOTER_SIZE);
            let data = &input[(input.len() - FOOTER_SIZE)..magic_start];

            let (checksum_type, data) = (ChecksumType::from_value(data[0])?, &data[1..]);
            let (metaindex_handle, data) = BlockHandle::parse(data)?;
            let (index_handle, data) = BlockHandle::parse(data)?;

            let footer_version_start = data.len() - 4;
            check_padding(&data[0..footer_version_start])?;
            let footer_version = u32::from_le_bytes(*array_ref![data, footer_version_start, 4]);

            if footer_version == 0 {
                return Err(err_msg(
                    "Not allowed to have old footer version with new format",
                ));
            }

            Ok(Self {
                checksum_type,
                metaindex_handle,
                index_handle,
                footer_version,
            })
        } else if magic == BLOCK_BASED_MAGIC_LEGACY {
            min_size!(input, LEGACY_FOOTER_SIZE);
            let data = &input[(input.len() - LEGACY_FOOTER_SIZE)..magic_start];

            let (metaindex_handle, data) = BlockHandle::parse(data)?;
            let (index_handle, data) = BlockHandle::parse(data)?;
            check_padding(data)?;

            Ok(Self {
                checksum_type: ChecksumType::CRC32C,
                metaindex_handle,
                index_handle,
                footer_version: 0,
            })
        } else {
            return Err(err_msg("Incorrect magic"));
        }
    }

    pub fn serialize(&self, out: &mut Vec<u8>) {
        if self.footer_version == 0 {
            assert_eq!(self.checksum_type, ChecksumType::CRC32C);

            let start_index = out.len();
            self.metaindex_handle.serialize(out);
            self.index_handle.serialize(out);
            out.resize(start_index + 2 * BLOCK_HANDLE_MAX_SIZE, 0);
            out.extend_from_slice(&BLOCK_BASED_MAGIC_LEGACY.to_le_bytes());
        } else {
            out.push(self.checksum_type as u8);

            let start_index = out.len();
            self.metaindex_handle.serialize(out);
            self.index_handle.serialize(out);
            out.resize(start_index + 2 * BLOCK_HANDLE_MAX_SIZE, 0);

            out.extend_from_slice(&self.footer_version.to_le_bytes());
            out.extend_from_slice(&BLOCK_BASED_MAGIC.to_le_bytes());
        };
    }
}

enum_def!(ChecksumType u8 =>
    None = 0,
    CRC32C = 1,
    XXHash = 2,
    XXHash64 = 3
);
