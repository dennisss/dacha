use std::collections::HashMap;
use std::collections::VecDeque;
use std::future::Future;
use std::ops::{Deref, Index};
use std::path::Path;
use std::slice::SliceIndex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use common::algorithms::SliceLike;
use common::async_std::fs::File;
use common::async_std::io::prelude::{ReadExt, SeekExt, WriteExt};
use common::async_std::io::{Read, Seek, SeekFrom, Write};
use common::async_std::sync::Mutex;
use common::bytes::Bytes;
use common::errors::*;
use common::futures::channel::oneshot;
use parsing::complete;
use protobuf::wire::{parse_varint, serialize_varint};
use reflection::*;

use crate::iterable::Iterable;
use crate::iterable::KeyValueEntry;
use crate::table::block_handle::BlockHandle;
use crate::table::data_block::*;
use crate::table::filter_policy::FilterPolicyRegistry;
use crate::table::footer::*;
use crate::table::table_properties::*;

use super::comparator::KeyComparator;
use super::filter_block::FilterBlock;
use super::filter_policy::FilterPolicy;

// TODO: Unused
pub const METAINDEX_PROPERTIES_KEY: &'static str = "rocksdb.properties";

static NEXT_ID: AtomicUsize = AtomicUsize::new(0);

#[derive(Clone)]
pub struct SSTableOpenOptions {
    pub comparator: Arc<dyn KeyComparator>,
    // pub filter_factory: Arc<dyn FilterPolicy>,
}

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

    filter: Option<SSTableFilter>,

    comparator: Arc<dyn KeyComparator>,

    footer: Footer,
}

struct SSTableFilter {
    policy: Arc<dyn FilterPolicy>,
    block: FilterBlock,
}

/*
    Iterator must have reference to SSTable to
    - Capturing the
*/

impl SSTable {
    pub async fn open<P: AsRef<Path>>(path: P, options: SSTableOpenOptions) -> Result<Self> {
        Self::open_impl(path.as_ref(), options).await
    }

    async fn open_impl(path: &Path, options: SSTableOpenOptions) -> Result<Self> {
        // TODO: Validate that the entire file is accounted for in the block handles.

        let mut file = File::open(path).await?;

        let footer = Footer::read_from_file(&mut file).await?;

        let index = IndexBlock::create(
            &DataBlock::read(&mut file, &footer, &footer.index_handle)
                .await?
                .block(),
        )?;

        let metaindex = DataBlock::read(&mut file, &footer, &footer.metaindex_handle).await?;
        let mut iter = metaindex.block().iter().rows();

        let mut props = TableProperties::default();

        let mut filter = None;

        while let Some(res) = iter.next() {
            let row = res?;

            let name = std::str::from_utf8(row.key)?;
            let (handle, _) = parsing::complete(BlockHandle::parse)(row.value)?;

            // THe properties struct is here: https://github.com/facebook/rocksdb/blob/6c2bf9e916db3dc7a43c70f3c0a482b8f7d54bdf/include/rocksdb/table_properties.h#L142

            // NOTE: All properties are either strings or varint64
            // Full list is here: https://github.com/facebook/rocksdb/blob/9bd5fce6e89fcb294a1d193f32f3e4bb2e41d994/table/meta_blocks.cc#L74
            if name == METAINDEX_PROPERTIES_KEY {
                let block = DataBlock::read(&mut file, &footer, &handle).await?;
                props = Self::parse_properties(block.block())?;
                println!("{:?}", props);
            } else if let Some(filter_name) = name.strip_prefix("filter.") {
                if filter.is_some() {
                    return Err(err_msg("More than one filter in table"));
                }

                let filter_registry = FilterPolicyRegistry::default();

                let policy = filter_registry
                    .get(filter_name)
                    .ok_or_else(|| format_err!("Unknown filter with name: {}", filter_name))?;

                let block = FilterBlock::read(&mut file, &footer, &handle).await?;

                filter = Some(SSTableFilter { policy, block });

                continue;
            }

            eprintln!(
                "Unknown property key: {}",
                String::from_utf8(row.key.to_vec()).unwrap()
            );
        }

        // TODO: Find a better way to do this. Maybe better to require opening a table
        // with a block cache. That way, we just need to register a unique id
        // with that.
        let id = NEXT_ID.fetch_add(1, Ordering::SeqCst);

        Ok(Self {
            state: Arc::new(SSTableState {
                id,
                file: Mutex::new(file),
                index,
                properties: props,
                filter,
                comparator: options.comparator,
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

    fn parse_properties(block: &DataBlockRef) -> Result<TableProperties> {
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

/// Mapping from data key ranges to block handles that is used to find which
/// block contains a given search key.
///
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
    pub fn create(block: &DataBlockRef) -> Result<Self> {
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

    /// Looks up the index of the block containing the given key.
    pub fn lookup(&self, key: &[u8], comparator: &dyn KeyComparator) -> Option<usize> {
        common::algorithms::lower_bound_by(
            IndexBlockSlice {
                last_keys: &self.last_keys,
                last_key_offsets: &self.last_key_offsets,
            },
            key,
            |a, b| comparator.compare(a, b).is_ge(),
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
            // TODO: The main issue with this is that it only considers compressed size.
            // Ideally we would have an index of uncompressed sizes to better
            // keep track of memory usage.
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
                state.loaded_size += block.estimated_memory_usage() as u64;
                state.loaded_blocks.insert(key, block.clone());
                return Ok(DataBlockPtr {
                    cache_key: key,
                    inner: Some(block),
                    outer: Some(self.state.clone()),
                });
            }

            let (tx, rx) = oneshot::channel();
            state.change_listeners.push(tx);
            drop(state);
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
                outer_guard.loaded_size -= inner.estimated_memory_usage() as u64;
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

/// TODO: If we know that we are going to be scanning a large amount of the
/// table, implement read-ahead.
pub struct SSTableIterator {
    table: SSTable,

    block_cache: DataBlockCache,

    /// Index of the current block we are iterating on in the SSTable.
    current_block_index: usize,

    /// Reference to the above block. Used to
    current_block: Option<(DataBlockPtr, DataBlockKeyValueIterator<'static>)>,
    /* The upper bound for keys to return.
     * end_key: Option<&'a [u8]> */
}

// Simpler strategy is to copy it, but I'd like to avoid copying potentially

#[async_trait]
impl Iterable for SSTableIterator {
    /// TODO: Try to avoid copying.
    async fn next(&mut self) -> Result<Option<KeyValueEntry>> {
        loop {
            if let Some((block, block_iter)) = self.current_block.as_mut() {
                if let Some(res) = block_iter.next() {
                    let kv = res?;
                    return Ok(Some(KeyValueEntry {
                        key: kv.key.to_vec().into(),
                        value: kv.value.to_vec().into(),
                    }));
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
                        .await?;

                    let iter = block.block().iter().rows();

                    let iter = unsafe {
                        std::mem::transmute::<
                            DataBlockKeyValueIterator<'_>,
                            DataBlockKeyValueIterator<'static>,
                        >(iter)
                    };

                    self.current_block = Some((block, iter));
                    continue;
                }

                return Ok(None);
            }
        }
    }

    async fn seek(&mut self, start_key: &[u8]) -> Result<()> {
        // Find correct index
        let block_index = if let Some(idx) = self
            .table
            .state
            .index
            .lookup(start_key, self.table.state.comparator.as_ref())
        {
            idx
        } else {
            self.current_block_index = self.table.num_blocks();
            return Ok(());
        };

        // TODO: Optimize this if .current_block already has the block we want.
        let block = self
            .block_cache
            .get_or_create(&self.table, block_index)
            .await?;
        let iter = unsafe { std::mem::transmute::<_, &DataBlockRef<'static>>(block.block()) }
            .before(start_key, self.table.state.comparator.as_ref())?;

        self.current_block = Some((block, iter));
        Ok(())
    }

    /*
    /// Move the iterator to be immediately before a well known key value.
    ///
    /// If the key is definately not in the table, then this will return false
    /// and the iterator will be in an undefined position.
    ///
    /// If this returns true, then the next call to .next() will return the
    /// value at the given key (if it is present in the table).
    pub async fn seek_exact(&mut self, key: &[u8]) -> Result<bool> {
        // Find correct index
        // TODO: Deduplicate with above.
        let block_index = if let Some(idx) = self.table.state.index.lookup(start_key) {
            idx
        } else {
            self.current_block_index = self.table.num_blocks();
            return Ok(false);
        };

        // TODO: Check against any min-max key metadata in the table.

        if let Some(filter) = self.table.state.filter {
            if !filter
                .block
                .block()
                .key_may_match(filter.policy.as_ref(), block_index, key)
            {
                return Ok(false);
            }
        }

        // TODO: Next step is to check any block-level filters.
    }
    */
}
