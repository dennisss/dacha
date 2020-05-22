extern crate sstable;
extern crate common;

use common::errors::*;
use common::async_std::prelude::*;
use common::async_std::task;

async fn run() -> Result<()> {
	sstable::open_db("/home/dennis/workspace/dacha/testdata/leveldb").await?;
	// sstable::SSTable::open("/home/dennis/workspace/dacha/testdata/rocksdb/000007.sst").await?;

	Ok(())
}

fn main() -> Result<()> {
	task::block_on(run())
}