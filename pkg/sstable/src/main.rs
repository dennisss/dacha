#[macro_use]
extern crate common;
extern crate sstable;

use std::sync::Arc;

use common::async_std::prelude::*;
use common::async_std::task;
use common::bytes::Bytes;
use common::errors::*;
use sstable::iterable::Iterable;
use sstable::table::comparator::BytewiseComparator;
use sstable::table::table::DataBlockCache;
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

async fn run() -> Result<()> {
    let mut options = EmbeddedDBOptions::default();
    // options.create_if_missing = true;

    let db = EmbeddedDB::open(
        &project_path!("testdata/sstable/leveldb-food-mutate"),
        options,
    )
    .await?;

    let mut iter = db.snapshot().await.iter().await;

    while let Some(entry) = iter.next().await? {
        println!("{:?} => {:?}", entry.key, entry.value);
    }

    return Ok(());

    //	sstable::table::SSTable::open(project_path!("testdata/rocksdb/000004.sst")).
    // await?;

    /*
    let mut options = sstable::table::table::SSTableOpenOptions {
        comparator: Arc::new(BytewiseComparator::new()),
    };

    let table = sstable::table::table::SSTable::open(
        project_path!("testdata/sstable/leveldb-food/000005.ldb"),
        options,
    )
    .await?;

    let cache = DataBlockCache::new(10000000); // 10MB

    let mut iter = table.iter(&cache);

    while let Some(res) = iter.next().await {
        let (key, value) = res?;
        println!("{:?} => {:?}", Bytes::from(key), Bytes::from(value));
    }
    */

    Ok(())
}

fn main() -> Result<()> {
    task::block_on(run())
}
