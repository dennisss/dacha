use crate::block::{Block, BlockEntry};
use crate::block_builder::{BlockBuilder, UnsortedBlockBuilder};
use crate::comparator::*;
use crate::filter_block::FilterPolicy;
use crate::filter_block_builder::FilterBlockBuilder;
use crate::table::{BlockHandle, ChecksumType, CompressionType};
use common::async_std::fs::{File, OpenOptions};
use common::async_std::io::prelude::WriteExt;
use common::async_std::path::Path;
use common::errors::*;
use crypto::checksum::crc::CRC32CHasher;
use crypto::hasher::Hasher;
use std::cmp::Ordering;
use std::fs::metadata;
use std::sync::Arc;

#[derive(Defaultable)]
pub struct SSTableBuilderOptions {
    #[default(4096)]
    pub block_size: usize,
    #[default(16)]
    pub block_restart_interval: usize,

    /// If adding another entry to a block exceeds the block_size, if this
    /// percentage of the block is remaining, then start a new block.
    #[default(10)]
    pub block_size_deviation: usize,

    //	pub use_delta_encoding: bool,
    #[default(2)]
    pub format_version: u32,

    #[default(ChecksumType::CRC32C)]
    pub checksum: ChecksumType,

    #[default(CompressionType::Snappy)]
    pub compression: CompressionType,

    // TODO: Need to route this through to the block builders.
    #[default(Arc::new(BytewiseComparator::new()))]
    pub comparator: Arc<dyn Comparator>,

    // NOTE: We assume that whole_key_filtering is enabled.
    // TODO: Eventually we should make a format that saves whether or not it is
    // enabled.
    #[default(None)]
    pub filter_policy: Option<Arc<dyn FilterPolicy>>,

    /// Base for building
    /// NOTE: In LevelDB/RocksDB this is fixed to 11 as a constant in the code.
    #[default(11)]
    pub filter_log_base: u8, // default 11

    /// If the given table path already exists, truncate it and start again.
    /// When being used as part of a managed database, this should always be
    /// set to false.
    /// TODO: The main issue is that we would need to record the change to the
    /// file sequence number before starting to write new files.
    /// TODO: Implement this below
    #[default(false)]
    pub overwrite_existing: bool,
}

/*
    Restart interval:
    - For meta index: 1
    - For properties: max int
    - Range deletions block: 1
    -


    LevelDB options:
    - max_open_files: 1000
    - block_size: 4096
    - block_restart_interval: 16
    - compression: snappy

    // For keys
    use_delta_encoding: true


    block_size_deviation: 10
*/
struct PendingDataBlock {
    last_key: Vec<u8>,
    handle: BlockHandle,
}

// TODO: Must implement index_key_is_user_key so that the index stores the user
// keys without the sequence (as long as no snapshot saving is required).
// I also need to do the same for the bloom filter.

pub struct SSTableBuilder {
    file: File,
    file_len: usize,
    options: SSTableBuilderOptions,
    compressed_buffer: Vec<u8>,

    index_block_builder: BlockBuilder,
    //	data_block_handles: Vec<BlockHandle>,
    filter_block_builder: Option<FilterBlockBuilder>,
    data_block_builder: BlockBuilder,

    /// If set, that a data block was written to the file but hasn't been added
    /// to the
    pending_data_block: Option<PendingDataBlock>,
}

impl SSTableBuilder {
    pub async fn open(path: &Path, options: SSTableBuilderOptions) -> Result<Self> {
        // NOTE: We will panic if we are overwriting an existing table. This
        // should never
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .await?;

        let filter_block_builder = if let Some(policy) = options.filter_policy.clone() {
            Some(FilterBlockBuilder::new(policy, options.filter_log_base))
        } else {
            None
        };

        let data_block_builder = BlockBuilder::new(options.block_restart_interval);

        Ok(Self {
            file,
            file_len: 0,
            options,
            compressed_buffer: vec![],
            index_block_builder: BlockBuilder::new(1),
            filter_block_builder,
            data_block_builder,
            pending_data_block: None,
        })
    }

    async fn write_raw_block(&mut self, block_buffer: &mut Vec<u8>) -> Result<BlockHandle> {
        let compressed_data = match self.options.compression {
            // In the uncompressed case, we avoid copying over the data to a
            // separate buffer and reuse
            CompressionType::None => block_buffer,
            CompressionType::ZLib => {
                // TODO:
                &mut self.compressed_buffer
            }
            _ => {
                return Err(err_msg("Unsupported compression method"));
            }
        };

        let block_handle = BlockHandle {
            offset: self.file_len as u64,
            size: compressed_data.len() as u64,
        };

        compressed_data.push(self.options.compression as u8);

        let checksum: u32 = match self.options.checksum {
            ChecksumType::None => 0,
            ChecksumType::CRC32C => {
                let mut hasher = CRC32CHasher::new();
                hasher.update(compressed_data);
                hasher.masked()
            }
            _ => {
                return Err(err_msg("Unsupported checksum type"));
            }
        };

        compressed_data.extend_from_slice(&checksum.to_le_bytes());

        self.file.write(&compressed_data).await?;
        self.file_len += compressed_data.len();
        self.compressed_buffer.clear();
        Ok(block_handle)
    }

    /// Immediately writes any buffered keyed in the current block to disk.
    /// NOTE: The table will still not be readable until finished is called.
    pub async fn flush(&mut self) -> Result<()> {
        if self.data_block_builder.empty() {
            return Ok(());
        }

        let (mut block_buffer, last_key) = self.data_block_builder.finish();
        let block_handle = self.write_raw_block(&mut block_buffer).await?;
        self.data_block_builder.set_buffer(block_buffer);

        assert!(self.pending_data_block.is_none());
        self.pending_data_block = Some(PendingDataBlock {
            last_key,
            handle: block_handle,
        });

        if let Some(filter_builder) = self.filter_block_builder.as_mut() {
            filter_builder.start_block(self.file_len);
        }

        Ok(())
    }

    pub async fn add(&mut self, key: Vec<u8>, value: Vec<u8>) -> Result<()> {
        let min_cutoff = (self.options.block_size * self.options.block_size_deviation) / 100;

        // If we expect to overflow the block size, flush the flush the previous
        // block.
        // TODO: Check the edge cases of min_cutoff inequality.
        if (self.data_block_builder.projected_size(&key, &value) > self.options.block_size)
            && (self.data_block_builder.current_size() > min_cutoff)
        {
            self.flush().await?;
        }

        if let Some(pending) = self.pending_data_block.take() {
            // Ensure that the keys in this block are larger than the previous
            // block's keys.
            if self.options.comparator.compare(&pending.last_key, &key) != Ordering::Less {
                return Err(err_msg("Keys inserted out of order"));
            }

            let index_key = self
                .options
                .comparator
                .find_shortest_separator(pending.last_key, &key);
            self.index_block_builder
                .add(index_key, pending.handle.serialized())?;
        }

        self.data_block_builder.add(key.clone(), value)?;

        if let Some(filter_builder) = self.filter_block_builder.as_mut() {
            filter_builder.add_key(key);
        }

        Ok(())
    }

    // TODO: Add user api to add custom properties while building the table.

    /// Complete writing the table to disk. This should always be the final
    /// method called.
    pub async fn finish(mut self) -> Result<()> {
        // TODO: This check is also done inside of flush().
        self.flush().await?;

        if let Some(pending) = self.pending_data_block.take() {
            let index_key = self
                .options
                .comparator
                .find_short_successor(pending.last_key);
            self.index_block_builder
                .add(index_key, pending.handle.serialized())?;
        }

        // TODO: Make the interval configurable
        let mut metaindex_builder = UnsortedBlockBuilder::new(1);

        if let Some(filter_builder) = self.filter_block_builder.take() {
            let mut buffer = filter_builder.finish();
            let handle = self.write_raw_block(&mut buffer).await?;
            metaindex_builder.add(
                format!(
                    "filter.{}",
                    self.options.filter_policy.as_ref().unwrap().name()
                )
                .into_bytes(),
                handle.serialized(),
            );
        }

        let index_handle = {
            let (mut buf, _) = self.index_block_builder.finish();
            self.write_raw_block(&mut buf).await?
        };

        // TODO: Write properties here

        let metaindex_handle = {
            let mut buf = metaindex_builder.finish()?;
            self.write_raw_block(&mut buf).await?
        };

        // Write footer (using version in options).

        Ok(())
    }
}
