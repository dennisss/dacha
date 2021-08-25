#[macro_use]
extern crate common;
extern crate sstable;

use std::sync::Arc;

use common::async_std::prelude::*;
use common::async_std::task;
use common::bytes::Bytes;
use common::errors::*;
use sstable::table::comparator::BytewiseComparator;
use sstable::table::table::DataBlockCache;

async fn run() -> Result<()> {
    //	sstable::table::SSTable::open(project_path!("testdata/rocksdb/000004.sst")).
    // await?;

    let mut options = sstable::table::table::SSTableOpenOptions {
        comparator: Arc::new(BytewiseComparator::new()),
    };

    //	sstable::open_db(project_path!("testdata/leveldb")).await?;
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

    Ok(())
}

fn main() -> Result<()> {
    task::block_on(run())
}
