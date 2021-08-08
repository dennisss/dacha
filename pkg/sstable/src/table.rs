use crate::block::*;
use crate::table_properties::*;
use common::algorithms::SliceLike;
use common::async_std::fs::File;
use common::async_std::io::prelude::{ReadExt, SeekExt, WriteExt};
use common::async_std::io::{Read, Seek, SeekFrom, Write};
use common::async_std::sync::Mutex;
use common::bytes::Bytes;
use common::errors::*;
use common::futures::channel::oneshot;
use compression::snappy::*;
use crypto::checksum::crc::CRC32CHasher;
use crypto::hasher::Hasher;
use math::matrix::Dimension;
use parsing::complete;
use protobuf::wire::{parse_varint, serialize_varint};
use reflection::*;
use std::collections::HashMap;
use std::collections::VecDeque;
use std::future::Future;
use std::ops::{Deref, Index};
use std::path::Path;
use std::slice::SliceIndex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

// TODO: There are two different versions:
// https://github.com/facebook/rocksdb/blob/master/table/block_based/block_based_table_builder.cc#L208
const BLOCK_BASED_MAGIC: u64 = 0x88e241b785f4cff7;
/// This is compatible with LevelDB.
const BLOCK_BASED_MAGIC_LEGACY: u64 = 0xdb4775248b80fb57;
const MAGIC_SIZE: usize = 8;

const BLOCK_HANDLE_MAX_SIZE: usize = 20;

const LEGACY_FOOTER_SIZE: usize = 2 * BLOCK_HANDLE_MAX_SIZE + MAGIC_SIZE;
const FOOTER_SIZE: usize = 2 * BLOCK_HANDLE_MAX_SIZE + 1 + 4 + MAGIC_SIZE;

/// Always 1 byte for CompressionType + 4 bytes for checksum.
const BLOCK_TRAILER_SIZE: usize = 5;

pub const METAINDEX_PROPERTIES_KEY: &'static [u8] = b"rocksdb.properties";

const NEXT_ID: AtomicUsize = AtomicUsize::new(0);

// TODO: When in a database, we should have a common cache across all tables
// in the database.
#[derive(Clone)]
pub struct SSTable {
    state: Arc<SSTableState>,
}

struct SSTableState {
    /// Unique identifier representing
    id: usize,

    file: Mutex<File>,
    index: IndexBlock,
    properties: TableProperties,
    footer: Footer,
}

/*
    Iterator must have reference to SSTable to
    - Capturing the
*/

impl SSTable {
    pub async fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        Self::open_impl(path.as_ref()).await
    }

    async fn open_impl(path: &Path) -> Result<Self> {
        // TODO: Validate that the entire file is accounted for in the block handles.

        let mut file = File::open(path).await?;
        let metadata = file.metadata().await?;
        let len = metadata.len();
        if len < (FOOTER_SIZE as u64) {
            return Err(err_msg("File too small"));
        }

        let footer = {
            file.seek(SeekFrom::Start(len - (FOOTER_SIZE as u64)))
                .await?;
            let mut buf = [0u8; FOOTER_SIZE];
            file.read_exact(&mut buf).await?;
            Footer::parse(&buf)?
        };

        println!("{:#?}", footer);

        let index = IndexBlock::create(
            &DataBlock::read(&mut file, &footer, &footer.index_handle)
                .await?
                .block,
        )?;

        let metaindex = DataBlock::read(&mut file, &footer, &footer.metaindex_handle).await?;
        let mut iter = metaindex.block.before(b"")?.rows();

        let mut props = TableProperties::default();

        while let Some(res) = iter.next() {
            let row = res?;

            // TODO: Assert completely parsed.
            let (handle, _) = BlockHandle::parse(row.value)?;

            // THe properties struct is here: https://github.com/facebook/rocksdb/blob/6c2bf9e916db3dc7a43c70f3c0a482b8f7d54bdf/include/rocksdb/table_properties.h#L142

            // NOTE: All properties are either strings or varint64
            // Full list is here: https://github.com/facebook/rocksdb/blob/9bd5fce6e89fcb294a1d193f32f3e4bb2e41d994/table/meta_blocks.cc#L74
            if row.key == b"rocksdb.properties" {
                let block = DataBlock::read(&mut file, &footer, &handle).await?;
                props = Self::parse_properties(&block.block)?;
                println!("{:?}", props);
            }

            println!("{}", String::from_utf8(row.key.to_vec()).unwrap());
        }

        Ok(Self {
            state: Arc::new(SSTableState {
                id: NEXT_ID.fetch_add(1, Ordering::Relaxed),
                file: Mutex::new(file),
                index,
                properties: props,
                footer,
            }),
        })
    }

    pub fn iter(&self, block_cache: &DataBlockCache) -> SSTableIterator {
        SSTableIterator {
            table: self.clone(),
            block_cache: block_cache.clone(),
            current_block_index: 0,
            current_block: None,
        }
    }

    fn num_blocks(&self) -> usize {
        self.state.index.block_handles.len()
    }

    fn parse_properties(block: &Block) -> Result<TableProperties> {
        let mut pairs = HashMap::new();

        let mut iter = block.iter().rows();
        while let Some(res) = iter.next() {
            let row = res?;

            let key = String::from_utf8(row.key.to_vec())?;
            let value = row.value.to_vec();
            pairs.insert(key, value);
        }

        let mut props = TableProperties::default();

        for field_idx in 0..props.fields_len() {
            let field = props.fields_index_mut(field_idx);

            let name = if let Some(tag) = field.tags.iter().find(|t| t.key == "name") {
                tag.value
            } else {
                continue;
            };

            let value = if let Some(v) = pairs.remove(name) {
                v
            } else {
                continue;
            };

            match field.value {
                ReflectValue::U64(v) => *v = complete(parse_varint)(&value)?.0 as u64,
                ReflectValue::String(v) => {
                    // TODO: Ensure that this is always utf-8
                    *v = String::from_utf8(value.to_vec())?;
                }
                _ => return Err(err_msg("Unsupported properties field type")),
            };
        }

        if !pairs.is_empty() {
            println!("Unknown props: {:#?}", pairs);
        }

        Ok(props)
    }

    async fn read_block(&self, handle: &BlockHandle) -> Result<Arc<DataBlock>> {
        let mut file = self.state.file.lock().await;
        DataBlock::read(&mut file, &self.state.footer, handle).await
    }
}

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
            out.extend_from_slice(&BLOCK_BASED_MAGIC_LEGACY.to_be_bytes());
        } else {
            out.push(self.checksum_type as u8);

            let start_index = out.len();
            self.metaindex_handle.serialize(out);
            self.index_handle.serialize(out);
            out.resize(start_index + 2 * BLOCK_HANDLE_MAX_SIZE, 0);

            out.extend_from_slice(&self.footer_version.to_le_bytes());
            out.extend_from_slice(&BLOCK_BASED_MAGIC.to_be_bytes());
        };
    }
}

enum_def!(ChecksumType u8 =>
    None = 0,
    CRC32C = 1,
    XXHash = 2,
    XXHash64 = 3
);

#[derive(Debug)]
pub struct BlockHandle {
    pub offset: u64,
    pub size: u64,
}

impl BlockHandle {
    pub fn parse(input: &[u8]) -> Result<(Self, &[u8])> {
        let (offset, rest) = parse_varint(input)?;
        let (size, rest) = parse_varint(rest)?;
        Ok((
            Self {
                offset: offset as u64,
                size: size as u64,
            },
            rest,
        ))
    }

    pub fn serialize(&self, output: &mut Vec<u8>) {
        serialize_varint(self.offset, output);
        serialize_varint(self.size, output);
    }

    pub fn serialized(&self) -> Vec<u8> {
        let mut out = vec![];
        self.serialize(&mut out);
        out
    }
}

enum_def!(CompressionType u8 =>
    None = 0,
    Snappy = 1,
    ZLib = 2,
    BZip2 = 3,
    LZ4 = 4,
    LZ4HC = 5,
    XPress = 6,
    Zstd = 7
);

#[derive(Debug)]
struct RawBlock {
    data: Vec<u8>,
    compression_type: CompressionType, // is_data_block
}

impl RawBlock {
    async fn read(file: &mut File, footer: &Footer, handle: &BlockHandle) -> Result<Self> {
        let mut buf = vec![];
        file.seek(SeekFrom::Start(handle.offset)).await?;
        buf.resize((handle.size as usize) + BLOCK_TRAILER_SIZE, 0);
        file.read_exact(&mut buf).await?;

        min_size!(buf, BLOCK_TRAILER_SIZE);
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

    fn decompress(self) -> Result<Vec<u8>> {
        Ok(match self.compression_type {
            CompressionType::None => self.data,
            CompressionType::Snappy => {
                let mut out = vec![];
                compression::snappy::snappy_decompress(&self.data, &mut out)?;
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

fn check_padding(s: &[u8]) -> Result<()> {
    for b in s {
        if *b != 0 {
            return Err(err_msg("Non-zero padding"));
        }
    }

    Ok(())
}

/// Block format:
/// - Block contents
/// - Trailer:
/// 	- [0]: compression_type u8
/// 	- [1]: Checksum of [block_contents | compression_type]
/// 	- Padding (if a data block)
/// 		- (RocksDB will pad to a block size of 4096 by default)

/// Instantiated index block.
/// This is an optimized map<last_key, block_handle> where upper_key is >= the
/// last key in a data block and block_handle is a pointer to the block.
///
/// Keys are held in a contiguous array so that hopefully searches can stay
/// within a single cache block.
struct IndexBlock {
    last_keys: Vec<u8>,
    /// For each data block, this contains the offset into the last_keys buffer
    /// at which it's last key is stored.
    last_key_offsets: Vec<u32>,
    block_handles: Vec<BlockHandle>,
}

impl IndexBlock {
    pub fn create(block: &Block) -> Result<Self> {
        let mut last_keys = vec![];
        let mut last_key_offsets = vec![];
        let mut block_handles = vec![];

        let mut iter = block.iter().rows();
        while let Some(kv) = iter.next() {
            let kv = kv?;
            last_key_offsets.push(last_keys.len() as u32);
            last_keys.extend_from_slice(kv.key);
            block_handles.push(complete(BlockHandle::parse)(kv.value)?.0);
        }

        Ok(Self {
            last_keys,
            last_key_offsets,
            block_handles,
        })
    }

    /// Looks up the index of the block containing the given key
    pub fn lookup(&self, key: &[u8]) -> Option<usize> {
        common::algorithms::lower_bound_by(
            IndexBlockSlice {
                last_keys: &self.last_keys,
                last_key_offsets: &self.last_key_offsets,
            },
            key,
            |a, b| a >= b,
        )
    }
}

pub struct IndexBlockSlice<'a> {
    last_keys: &'a [u8],
    last_key_offsets: &'a [u32],
}

impl<'a> SliceLike for IndexBlockSlice<'a> {
    type Item = &'a [u8];

    fn len(&self) -> usize {
        self.last_key_offsets.len()
    }

    fn index(&self, idx: usize) -> Self::Item {
        let start = self.last_key_offsets[idx] as usize;
        let end = self
            .last_key_offsets
            .get(idx + 1)
            .cloned()
            .unwrap_or(self.last_keys.len() as u32) as usize;

        &self.last_keys[start..end]
    }

    fn slice(&self, start: usize, end: usize) -> Self {
        Self {
            last_keys: self.last_keys,
            last_key_offsets: &self.last_key_offsets[start..end],
        }
    }

    fn slice_from(&self, start: usize) -> Self {
        Self {
            last_keys: self.last_keys,
            last_key_offsets: &self.last_key_offsets[start..],
        }
    }
}

/*
    Things always in memory:
    - All metablocks
    - Index block
    - Recently used data blocks
    - File footer
    -

    For a block, we should be able to seek to a position in the block and
*/

// In index, find last block with key >= query key
// - Because the index is static, we will store it as a single Vec<(&'[u8],
//   BlockHandle)>
// - Ideally as much of the keys will be in a single contiguous memory buffer
//   for better performance.

/*
    Iterating on the SSTable level:
    - We would need a reference to the SSTable to gurantee that the SSTable
      outlives the iterator.

    - Given a reference to the SSTable, an iterator references a single Block

    So the iterator is:
    -
*/

/// NOTE: After creation, a DataBlock is immutable.
/// TODO: Try doing something like https://stackoverflow.com/questions/23743566/how-can-i-force-a-structs-field-to-always-be-immutable-in-rust to force the immutability. (in particular, the Vec<u8> should never be allowed to be moved.
pub struct DataBlock {
    uncompressed: Vec<u8>,
    block: Block<'static>,
}

impl DataBlock {
    /// NOTE: This is the only safe way to create a DataBlock
    pub fn parse(data: Vec<u8>) -> Result<Arc<Self>> {
        let ptr: &'static [u8] = unsafe { std::mem::transmute::<&[u8], _>(&data) };
        let block = Block::parse(ptr)?;
        Ok(Arc::new(Self {
            uncompressed: data,
            block,
        }))
    }

    /// TODO: For the index, we don't need the Arc as we will immediately cast
    /// to a different format.
    async fn read(file: &mut File, footer: &Footer, handle: &BlockHandle) -> Result<Arc<Self>> {
        let raw = RawBlock::read(file, footer, handle).await?;
        let data = raw.decompress()?;
        let block = Self::parse(data)?;
        Ok(block)
    }
}

///
/// NOTE: Cloning this will refer to the same internal state.
///
///
/// TODO: This should be able to make hard gurantees on memory usage. It should
/// only drop a block once we know that all references to that block are dead.
/// Thus, loading from the cache can be seen as a blocking operation in the
/// case that we have loaded beyond the max amount (possibly have Arc
/// dereferences go through a channel when they hit zero).
///
/// TODO: Ideally generalize this so that many things can be tracked with it.
#[derive(Clone)]
pub struct DataBlockCache {
    state: Arc<Mutex<DataBlockCacheState>>,
}

impl DataBlockCache {
    pub fn new(allowed_size: u64) -> Self {
        Self {
            state: Arc::new(Mutex::new(DataBlockCacheState {
                loaded_blocks: HashMap::new(),
                change_listeners: vec![],
                loaded_size: 0,
                allowed_size,
            })),
        }
    }

    async fn get_or_create(&self, table: &SSTable, block_index: usize) -> Result<DataBlockPtr> {
        let key = (table.state.id, block_index);
        loop {
            let block_size = table.state.index.block_handles[block_index].size;

            let mut state = self.state.lock().await;
            if let Some(value) = state.loaded_blocks.remove(&key) {
                return Ok(DataBlockPtr {
                    cache_key: key,
                    inner: Some(value.clone()),
                    outer: Some(self.state.clone()),
                });
            }

            if block_size > state.allowed_size {
                return Err(err_msg("Fetching block larger than max cache size"));
            }

            if state.loaded_size + block_size <= state.allowed_size {
                let block = table
                    .read_block(&table.state.index.block_handles[block_index])
                    .await?;
                state.loaded_size += block.uncompressed.len() as u64;
                state.loaded_blocks.insert(key, block.clone());
                return Ok(DataBlockPtr {
                    cache_key: key,
                    inner: Some(block),
                    outer: Some(self.state.clone()),
                });
            }

            let (tx, rx) = oneshot::channel();
            state.change_listeners.push(tx);
            rx.await.ok();
        }
    }
}

struct DataBlockCacheState {
    /// All blocks currently loaded into memory.
    /// The key is of the form (table_id, block_index).
    loaded_blocks: HashMap<(usize, usize), Arc<DataBlock>>,

    change_listeners: Vec<oneshot::Sender<()>>,

    /// Number of bytes
    loaded_size: u64,

    allowed_size: u64,
}

/// Reference counted pointer to a DataBlock in a DataBlockCache.
/// This functions effectively the same as a Arc<DataBlock>, except will free up
/// space in the cache when it is dropped.
struct DataBlockPtr {
    cache_key: (usize, usize),
    inner: Option<Arc<DataBlock>>,
    outer: Option<Arc<Mutex<DataBlockCacheState>>>,
}

impl Deref for DataBlockPtr {
    type Target = DataBlock;

    fn deref(&self) -> &Self::Target {
        &self.inner.as_ref().unwrap()
    }
}

impl Drop for DataBlockPtr {
    fn drop(&mut self) {
        let cache_key = self.cache_key;
        let inner = self.inner.take().unwrap();
        let outer = self.outer.take().unwrap();
        common::async_std::task::spawn(async move {
            let mut outer_guard = outer.lock().await;
            let count = Arc::strong_count(&inner);

            // If exactly one in the current context and one in the cache, then we can drop
            // it.
            if count == 2 {
                outer_guard.loaded_blocks.remove(&cache_key);
                outer_guard.loaded_size -= inner.uncompressed.len() as u64;
                drop(inner);

                // Notify tasks waiting to create a block.
                // NOTE: Because we are still holding a lock in 'outer_guard', the listeners
                // won't start executing until this task is finished.
                //
                // TODO: Eventually schedule the minimum number of tasks depending the amount of
                // memory freed and the amount requested by tasks (there may also be multiple
                // tasks which all request the same block).
                while let Some(sender) = outer_guard.change_listeners.pop() {
                    sender.send(()).ok();
                }
            }
        });
    }
}

pub struct SSTableIterator {
    table: SSTable,

    block_cache: DataBlockCache,

    /// Index of the current block we are iterating on in the SSTable.
    current_block_index: usize,

    /// Reference to the above block. Used to
    current_block: Option<(DataBlockPtr, BlockKeyValueIterator<'static>)>,
    /* The upper bound for keys to return.
     * end_key: Option<&'a [u8]> */
}

// Simpler strategy is to copy it, but I'd like to avoid copying potentially

// TODO: Uncomment this.
/*
impl SSTableIterator {
    async fn next<'a>(&'a mut self) -> Option<Result<KeyValuePair<'a>>> {
        loop {
            if self.current_block.is_some() {
                if let Some(res) = self.current_block.as_mut().unwrap().1.next() {
                    return Some(res);
                } else {
                    self.current_block = None;
                    self.current_block_index += 1;
                }
            } else {
                if self.current_block_index < self.table.num_blocks() {
                    // Get a new block and put it into the cache.
                    let block = self
                        .block_cache
                        .get_or_create(&self.table, self.current_block_index)
                        .await;
                    let block = match block {
                        Ok(b) => b,
                        Err(e) => {
                            return Some(Err(e));
                        }
                    };

                    let iter = block.block.iter().rows();

                    let iter = unsafe {
                        std::mem::transmute::<
                            BlockKeyValueIterator<'_>,
                            BlockKeyValueIterator<'static>,
                        >(iter)
                    };

                    self.current_block = Some((block, iter));
                }

                return None;
            }
        }
    }

    async fn seek(&mut self, start_key: &[u8]) -> Result<()> {
        // Find correct index
        let block_index = if let Some(idx) = self.table.state.index.lookup(start_key) {
            idx
        } else {
            self.current_block_index = self.table.num_blocks();
            return Ok(());
        };

        let block = self
            .block_cache
            .get_or_create(&self.table, block_index)
            .await?;
        let iter = block.block.before(start_key)?;

        // TODO: Iterate until at least at the key (would need to be able to peek)

        self.current_block = Some((block, iter));
        Ok(())
    }
}
*/