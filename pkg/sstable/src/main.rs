#[macro_use]
extern crate common;
extern crate sstable;

use std::collections::HashMap;
use std::collections::HashSet;
use std::num;
use std::sync::Arc;
use std::time::SystemTime;

use common::async_std::path::{Path, PathBuf};
use common::async_std::prelude::*;
use common::async_std::task;
use common::bytes::Bytes;
use common::errors::*;
use common::temp::TempDir;
use sstable::iterable::Iterable;
use sstable::table::comparator::BytewiseComparator;
use sstable::table::table::DataBlockCache;
use sstable::table::CompressionType;
use sstable::{EmbeddedDB, EmbeddedDBOptions};

/*
LevelDB
- max number of levels: 7
- max level of the memtable: 2

When we perform a write (or initially create the DB),
- If there is no immutable memtable and the memtable is over the size limit
- Atomically:
    - Create new memtable
    - Switch to a new log and set the manifest (prev and current log number in one record)
        - Also increase the next file number in the manifest
- Become building the memtable file in the directory
- Once done, unset the immutable table and mark no prev log in manifest (and add the file to the version)

Algorithm for compacting file on disk:
-

- Level 0 compaction triggered by a target number of files
    - These need to be ordered

- Trivial compaction if there is no overlap with lower layers
- Every level other than 0 is range partitioned
- Each level has a target size (one or more slices are plucked to be merged down if )

*/

async fn test_table() -> Result<()> {
    let cache = DataBlockCache::new(10000000); // 10MB

    let mut options = sstable::table::table::SSTableOpenOptions {
        comparator: Arc::new(BytewiseComparator::new()),
        block_cache: cache,
    };

    let table =
        sstable::table::table::SSTable::open(project_path!("out/big-sstable/000007.ldb"), options)
            .await?;

    let mut iter = table.iter();

    while let Some(res) = iter.next().await? {
        println!("{:?} => {:?}", res.key, res.value);
        break;
    }

    Ok(())
}

async fn run() -> Result<()> {
    let mut options = EmbeddedDBOptions::default();
    options.read_only = true;

    let mut db = EmbeddedDB::open(Path::new("/tmp/dacha/1630808067193796771"), options).await?;

    let mut iter = db.snapshot().await.iter().await?;

    while let Some(entry) = iter.next().await? {
        println!("{:?} => {:?}", entry.key, entry.value);
    }

    // 01700000\

    //	sstable::table::SSTable::open(project_path!("testdata/rocksdb/000004.sst")).
    // await?;

    Ok(())
}

fn main() -> Result<()> {
    task::block_on(run())
}
