extern crate common;
extern crate sstable;

use common::async_std::prelude::*;
use common::async_std::task;
use common::errors::*;

async fn run() -> Result<()> {
    //	sstable::table::SSTable::open(project_path!("testdata/rocksdb/000004.sst")).await?;

    //	sstable::open_db(project_path!("testdata/leveldb")).await?;
    sstable::table::SSTable::open(project_path!("testdata/rocksdb/000007.sst"))
        .await?;

    Ok(())
}

fn main() -> Result<()> {
    task::block_on(run())
}
