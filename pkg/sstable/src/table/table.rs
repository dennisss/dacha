use std::collections::HashMap;
use std::collections::HashSet;
use std::collections::VecDeque;
use std::future::Future;
use std::ops::{Deref, Index};
use std::slice::SliceIndex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use common::algorithms::SliceLike;
use common::bytes::Bytes;
use common::errors::*;
use executor::oneshot;
use executor::sync::Mutex;
use file::LocalFile;
use file::LocalPath;
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

pub const METAINDEX_PROPERTIES_KEY: &'static str = "rocksdb.properties";

#[derive(Clone)]
pub struct SSTableOpenOptions {
    pub comparator: Arc<dyn KeyComparator>,
    pub block_cache: DataBlockCache, // pub filter_factory: Arc<dyn FilterPolicy>,
}

// TODO: When an SSTable is dropped, we can clear all of the blocks from the
// cache.

// TODO: When in a database, we should have a common cache across all tables
// in the database.
#[derive(Clone)]
pub struct SSTable {
    state: Arc<SSTableState>,
}

struct SSTableState {
    file: BlockCacheFile,

    index: IndexBlock,

    properties: TableProperties,

    filter: Option<SSTableFilter>,

    comparator: Arc<dyn KeyComparator>,
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
    pub async fn open<P: AsRef<LocalPath>>(path: P, options: SSTableOpenOptions) -> Result<Self> {
        Self::open_impl(path.as_ref(), options).await
    }

    async fn open_impl(path: &LocalPath, options: SSTableOpenOptions) -> Result<Self> {
        // TODO: Use the block cache for opening, but don't block on the cache being
        // available (or track if we run out of space for sstable core blocks).

        // TODO: Validate that the entire file is accounted for in the block handles.

        let mut file = LocalFile::open(path)?;

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

        Ok(Self {
            state: Arc::new(SSTableState {
                file: options.block_cache.cache_file(file, footer).await,
                index,
                properties: props,
                filter,
                comparator: options.comparator,
            }),
        })
    }

    pub fn iter(&self) -> SSTableIterator {
        SSTableIterator {
            table: self.clone(),
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
                ReflectValue::U64(v) => {
                    *v = complete(|input| parse_varint(input).map_err(|e| Error::from(e)))(&value)?
                        .0 as u64
                }
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
/// TODO: Ideally generalize this so that many things can be tracked with it in
/// addition to non-DataBlock blocks
#[derive(Clone)]
pub struct DataBlockCache {
    state: Arc<Mutex<DataBlockCacheState>>,
}

impl DataBlockCache {
    pub fn new(allowed_size: usize) -> Self {
        Self {
            state: Arc::new(Mutex::new(DataBlockCacheState {
                last_table_id: 0,
                loaded_blocks: HashMap::new(),
                unused_blocks: HashSet::new(),
                change_listeners: vec![],
                loaded_size: 0,
                allowed_size,
            })),
        }
    }

    pub async fn cache_file(self, file: LocalFile, file_footer: Footer) -> BlockCacheFile {
        let id = {
            let mut state = self.state.lock().await;
            let id = state.last_table_id + 1;
            state.last_table_id = id;
            id
        };

        BlockCacheFile {
            id,
            cache: self,
            file: Mutex::new(file),
            file_footer,
        }
    }
}

pub struct BlockCacheFile {
    id: usize,
    cache: DataBlockCache,
    file: Mutex<LocalFile>,
    file_footer: Footer,
}

impl BlockCacheFile {
    async fn lookup_or_read(&self, block_handle: &BlockHandle) -> Result<DataBlockPtr> {
        let key = (self.id, block_handle.offset);
        loop {
            // TODO: The main issue with this is that it only considers compressed size.
            // Ideally we would have an index of uncompressed sizes to better
            // keep track of memory usage.
            let block_size = block_handle.size as usize;

            let mut state = self.cache.state.lock().await;
            if let Some(value) = state.loaded_blocks.get(&key) {
                return Ok(DataBlockPtr {
                    cache_key: key,
                    inner: Some(value.clone()),
                    outer: Some(self.cache.state.clone()),
                });
            }

            if block_size > state.allowed_size {
                return Err(err_msg("Fetching block larger than max cache size"));
            }

            // If we are out of space, free unused blocks if there are any.
            while state.loaded_size + block_size >= state.allowed_size {
                if let Some(block_key) = state.unused_blocks.iter().next().cloned() {
                    state.unused_blocks.remove(&block_key);

                    let block = state.loaded_blocks.remove(&block_key).unwrap();
                    state.loaded_size -= block.estimated_memory_usage();
                } else {
                    break;
                }
            }

            if state.loaded_size + block_size <= state.allowed_size {
                let block = self.read_block(block_handle).await?;
                state.loaded_size += block.estimated_memory_usage();
                state.loaded_blocks.insert(key, block.clone());
                return Ok(DataBlockPtr {
                    cache_key: key,
                    inner: Some(block),
                    outer: Some(self.cache.state.clone()),
                });
            }

            let (tx, rx) = oneshot::channel();
            state.change_listeners.push(tx);
            drop(state);
            rx.recv().await.ok();
        }
    }

    async fn read_block(&self, handle: &BlockHandle) -> Result<Arc<DataBlock>> {
        let mut file = self.file.lock().await;
        DataBlock::read(&mut file, &self.file_footer, handle).await
    }
}

struct DataBlockCacheState {
    /// Id of the last table assigned to this cache.
    last_table_id: usize,

    /// All blocks currently loaded into memory.
    /// The key is of the form (table_id, block_offset).
    loaded_blocks: HashMap<(usize, u64), Arc<DataBlock>>,

    /// Subset of keys in loaded_blocks which correspond to blocks which are
    /// loaded into memory but have no external references. If we are out of
    /// memory we will delete entries from this set.
    ///
    /// TODO: Refactor so that we delete blocks in LRU order.
    unused_blocks: HashSet<(usize, u64)>,

    // Ideally have a linked list so that we can quickly un-delete a
    change_listeners: Vec<oneshot::Sender<()>>,

    /// Number of bytes used by all blocks in loaded_blocks.
    loaded_size: usize,

    /// Maximum number of bytes to keep in memory before we start freeing unused
    /// blocks / blocking.
    allowed_size: usize,
}

/// Reference counted pointer to a DataBlock in a DataBlockCache.
/// This functions effectively the same as a Arc<DataBlock>, except will free up
/// space in the cache when it is dropped.
struct DataBlockPtr {
    cache_key: (usize, u64),
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
        // NOTE: This must run till completion always
        executor::spawn(async move {
            let mut outer_guard = outer.lock().await;
            let count = Arc::strong_count(&inner);
            drop(inner);

            // If exactly one in the current context and one in the cache, then we can drop
            // it.
            if count == 2 {
                outer_guard.unused_blocks.insert(cache_key);

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

    /// Index of the current block we are iterating on in the SSTable.
    current_block_index: usize,

    /// Reference to the above block. Used to
    current_block: Option<(DataBlockPtr, DataBlockKeyValueIterator<'static>)>,
    /* The upper bound for keys to return.
     * end_key: Option<&'a [u8]> */
}

// Simpler strategy is to copy it, but I'd like to avoid copying potentially

#[async_trait]
impl Iterable<KeyValueEntry> for SSTableIterator {
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
                    let block_handle =
                        &self.table.state.index.block_handles[self.current_block_index];

                    // Get a new block and put it into the cache.
                    let block = self.table.state.file.lookup_or_read(block_handle).await?;

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
            self.current_block = None;
            self.current_block_index = self.table.num_blocks();
            return Ok(());
        };

        let block_handle = &self.table.state.index.block_handles[block_index];

        // TODO: Optimize this if .current_block already has the block we want.
        let block = self.table.state.file.lookup_or_read(block_handle).await?;
        let iter = unsafe { std::mem::transmute::<_, &DataBlockRef<'static>>(block.block()) }
            .before(start_key, self.table.state.comparator.as_ref())?;

        self.current_block = Some((block, iter));
        self.current_block_index = block_index;
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

#[cfg(test)]
mod tests {

    use crypto::random::{self, Rng};
    use file::temp::TempDir;

    use crate::table::{table_builder::*, BytewiseComparator};

    use super::*;

    #[testcase]
    async fn sstable_build_and_seek() -> Result<()> {
        let dir = TempDir::create()?;
        let table_path = dir.path().join("table");

        let options = SSTableBuilderOptions::default();
        let mut builder = SSTableBuilder::open(&table_path, options).await?;

        let mut keys = vec![];
        let mut values = vec![];

        let mut rng = random::clocked_rng();

        for i in 0..10000 {
            let mut key = format!("{:08}", i);

            let mut value = vec![0u8; 20];
            rng.generate_bytes(&mut value);

            builder.add(key.as_bytes(), &value).await?;

            keys.push(key);
            values.push(value);
        }

        builder.finish().await?;

        let block_cache = DataBlockCache::new(32000);

        let open_options = SSTableOpenOptions {
            comparator: Arc::new(BytewiseComparator::new()),
            block_cache,
        };

        let table = SSTable::open(&table_path, open_options).await?;

        {
            let mut iter = table.iter();
            for i in 0..keys.len() {
                let entry = iter.next().await?.unwrap();
                assert_eq!(&entry.key, keys[i].as_bytes());
                assert_eq!(&entry.value, &values[i]);
            }

            assert!(iter.next().await?.is_none());
        }

        // Key is beyond the end of the table.
        {
            let mut iter = table.iter();
            iter.seek(&[b'0', b'1']).await?;
            assert!(iter.next().await?.is_none());
        }

        // Seeking to key in the middle of the table.
        for start_i in [1, 1000, 2000, 5000] {
            let mut iter = table.iter();
            iter.seek(keys[start_i].as_bytes()).await?;

            for i in start_i..keys.len() {
                let entry = iter.next().await?.unwrap();
                assert_eq!(&entry.key, keys[i].as_bytes());
                assert_eq!(&entry.value, &values[i]);
            }

            assert!(iter.next().await?.is_none());
        }

        // Seeking around the table multiple times.
        // TODO: Also test with in-exact seeking to keys in-between existing keys.
        {
            let mut iter = table.iter();
            for i in [10, 1, 600, 1000, 601, 5000, 8000, 702] {
                iter.seek(keys[i].as_bytes()).await?;

                let entry = iter.next().await?.unwrap();
                assert_eq!(&entry.key, keys[i].as_bytes());
                assert_eq!(&entry.value, &values[i]);
            }
        }

        // TODO: Test re-using the same iterator to seek multiple times or seek after
        // entries have already been read.

        Ok(())
    }
}
