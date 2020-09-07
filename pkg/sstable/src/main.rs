extern crate common;
extern crate sstable;

use common::async_std::prelude::*;
use common::async_std::task;
use common::errors::*;

async fn run() -> Result<()> {
    //	sstable::table::SSTable::open("/home/dennis/workspace/dacha/testdata/rocksdb/
    // 000004.sst").await?;

    //	sstable::open_db("/home/dennis/workspace/dacha/testdata/leveldb").await?;
    sstable::table::SSTable::open("/home/dennis/workspace/dacha/testdata/rocksdb/000007.sst")
        .await?;

    Ok(())
}

fn main() -> Result<()> {
    task::block_on(run())
}
