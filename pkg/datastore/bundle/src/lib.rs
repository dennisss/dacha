use alloc::boxed::Box;
use alloc::vec::Vec;

use common::async_std::future::pending;
use common::errors::*;
use common::io::{Readable, Writeable};
use compression::transform::Transform;
use compression::zlib::ZlibEncoder;
use crypto::hasher::Hasher;
use file::{LocalFile, LocalFileOpenOptions, LocalPath};
use protobuf::Message;

use crate::proto::bundle::*;

const BUNDLE_SHARD_MAGIC: &'static [u8] = b"daBS";
const DEFAULT_ALIGNMENT: u64 = 4096;
const DEFAULT_RECORD_SIZE: u64 = 256 * 1024;
const LARGE_FILE_THRESHOLD: u64 = 4 * DEFAULT_RECORD_SIZE;

pub mod bundle {
    include!(concat!(env!("OUT_DIR"), "/src/proto/bundle.rs"));
}

/*
I need some of the database utilities for this.
*/

pub struct SingleFileBundleWriter {
    // Maintain a memtable with all the records generate.

    //
}

impl SingleFileBundleWriter {
    // add()

    // finish()
}

struct BundleShardWriter {
    header: BundleShardHeader,

    file: LocalFile,

    /// Offset from the start of the shard file at which data begins.
    /// All compressed data offsets are defined relative to this position
    data_offset: u64,

    /// Current position in the file at which we are writing.
    file_position: u64,

    ///
    uncompressed_position: u64,

    pending_record: Option<PendingRecord>,
}

struct PendingRecord {
    compressor: Box<dyn Transform>,

    /// Number of uncompressed bytes seen so far.
    uncompressed_length: u64,

    /// Compressed data accumulated so far.
    buffer: Vec<u8>,

    /// If true, all uncompressed bytes in this record have been zeros.
    all_zeros: bool,
}

impl BundleShardWriter {
    pub async fn create(path: &LocalPath, header: BundleShardHeader) -> Result<Self> {
        let mut file =
            LocalFile::open_with_options(path, LocalFileOpenOptions::new().create_new(true))?;

        let mut first_block = vec![];
        first_block.extend_from_slice(BUNDLE_SHARD_MAGIC);

        let header_data = header.serialize()?;
        first_block.extend_from_slice(&(header_data.len() as u32).to_le_bytes());
        first_block.extend_from_slice(&(0u32).to_le_bytes());
        first_block.extend_from_slice(&header_data);

        let padded_len = first_block.len()
            + (common::block_size_remainder(header.record_alignment(), first_block.len() as u64)
                as usize);
        first_block.resize(padded_len, 0);

        // Store the data offset
        let data_offset = padded_len;
        first_block[8..12].copy_from_slice(&(padded_len as u32).to_le_bytes());

        file.write_all(&first_block).await?;

        Ok(Self {
            file,
            file_position: first_block.len() as u64,
            uncompressed_position: 0,
            data_offset: data_offset as u64,
            header,
            pending_record: None,
        })
    }

    /// NOTE: The returned BundleRecordMetadata list may not be complete and may
    /// relate to multiple files.
    pub async fn add(
        &mut self,
        mut data: Box<dyn Readable>,
        length: u64,
    ) -> Result<(BundleFileMetadata, Vec<BundleRecordMetadata>)> {
        let mut new_record_metas = vec![];

        if length >= LARGE_FILE_THRESHOLD {
            if let Some(pending_record) = self.pending_record.take() {
                new_record_metas.push(self.append_record(pending_record).await?);
            }
        }

        let mut file_meta = BundleFileMetadata::default();
        file_meta.set_shard_index(self.header.shard_index());
        file_meta.set_shard_uncompressed_offset(self.uncompressed_position);
        file_meta.set_uncompressed_size(length);

        let mut hasher = crypto::checksum::crc::CRC32CHasher::new();

        let mut n_read = 0;
        // TODO: Pick an optimal size for reading based on the input stream filesystem.
        let mut buffer = vec![0u8; 4 * 4096];

        loop {
            let mut n = data.read(&mut buffer).await?;
            n_read += n as u64;

            let mut remaining = &buffer[0..n];
            if n == 0 {
                break;
            }

            while !remaining.is_empty() {
                let mut pending_record = {
                    if let Some(pending_record) = self.pending_record.take() {
                        pending_record
                    } else {
                        self.new_record()?
                    }
                };

                let n = core::cmp::min(
                    remaining.len(),
                    (self.header.record_size() - pending_record.uncompressed_length) as usize,
                );

                compression::transform::transform_to_vec(
                    pending_record.compressor.as_mut(),
                    &remaining[0..n],
                    false,
                    &mut pending_record.buffer,
                )?;
                pending_record.uncompressed_length += n as u64;

                hasher.update(&remaining[0..n]);

                // TODO: Optimize out the result allocation here.
                pending_record.all_zeros &= common::check_zero_padding(&remaining[0..n]).is_ok();

                remaining = &remaining[n..];

                if pending_record.uncompressed_length == self.header.record_size() {
                    new_record_metas.push(self.append_record(pending_record).await?);
                } else {
                    self.pending_record = Some(pending_record);
                }
            }
        }

        if n_read != length {
            return Err(err_msg("Read wrong number of bytes for file"));
        }

        file_meta.set_crc32c(hasher.finish_u32());

        Ok((file_meta, new_record_metas))
    }

    fn new_record(&self) -> Result<PendingRecord> {
        let compressor = match self.header.compression_method() {
            BundleShardHeader_CompressionType::UNKNOWN => {
                todo!()
            }
            BundleShardHeader_CompressionType::NONE => {
                todo!()
            }
            BundleShardHeader_CompressionType::SNAPPY => {
                todo!()
            }
            BundleShardHeader_CompressionType::ZLIB => Box::new(ZlibEncoder::new()),
        };

        Ok(PendingRecord {
            compressor,
            uncompressed_length: 0,
            buffer: vec![],
            all_zeros: (),
        })
    }

    /// Finish building the given record and write it to the end of the shard
    /// file.
    async fn append_record(&mut self, mut record: PendingRecord) -> Result<BundleRecordMetadata> {
        if record.uncompressed_length == 0 {
            return Err(err_msg("Finishing empty record"));
        }

        let mut meta = BundleRecordMetadata::default();
        meta.set_shard_index(self.header.shard_index());
        meta.set_shard_uncompressed_offset(self.uncompressed_position);

        if record.all_zeros {
            meta.set_all_zeros(true);
        } else {
            // Flush pending compression state by signaling the end of inputs.
            compression::transform::transform_to_vec(
                record.compressor.as_mut(),
                &[],
                true,
                &mut record.buffer,
            )?;

            // Pad up to alignment
            let padded_len = record.buffer.len()
                + (common::block_size_remainder(
                    self.header.record_alignment(),
                    record.buffer.len() as u64,
                ) as usize);
            record.buffer.resize(padded_len, 0);

            meta.set_shard_byte_offset(self.file_position);
            self.file.write_all(&record.buffer).await?;
            self.file_position += record.buffer.len() as u64;
        }

        self.uncompressed_position += record.uncompressed_length as u64;

        Ok(meta)
    }

    /// Should be called exactly once after all write() calls.
    pub async fn finish_data(&mut self) -> Result<Option<BundleRecordMetadata>> {
        let mut last_record = None;
        if let Some(record) = self.pending_record.take() {
            last_record = Some(self.append_record(record).await?);
        }

        Ok(last_record)
    }

    pub async fn write_metadata_trailer(&mut self, table: &BundleInlineTable) -> Result<()> {
        // Write to the end of the file.

        todo!()
    }

    pub async fn flush(&mut self) -> Result<()> {
        self.file.flush().await
    }

    // Write from bounded stream.
    // => Will return
    // => If the config had an in-inline table, validate it?
    // TODO: Use plain storage when the data isn't compresable.

    // Finish
    // => (optionally writing some end table)
}
