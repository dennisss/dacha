#[macro_use]
extern crate common;
extern crate sstable;

use std::collections::HashMap;
use std::collections::HashSet;
use std::num;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::SystemTime;

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

struct TestDB {
    dir: TempDir,
    db: EmbeddedDB,
    values: HashMap<Bytes, Bytes>,
}

impl TestDB {
    async fn open() -> Result<Self> {
        let dir = TempDir::create()?;

        let mut options = EmbeddedDBOptions::default();
        options.create_if_missing = true;
        options.error_if_exists = true;
        // Disable compression to make it easier to predict compactions.
        options.table_options.compression = CompressionType::None;

        // Level 0 max size will be 20KB
        options.level0_file_num_compaction_trigger = 2;
        options.write_buffer_size = 10 * 1024; // 10KB

        // Make slightly larger than the write buffer to ensure that we don't create
        // tiny files with the overflow after accounting for file overhead.
        options.target_file_size_base = 12 * 1024; // 12KB
        options.target_file_size_multiplier = 1;

        // Level 1 max will be 20 KB (~4 files)
        // Level 2 max will be 40 KB (~4 files)
        // Level 3 max will be 80 KB (~8 files)
        options.max_bytes_for_level_base = 20 * 1024; // 20KB
        options.max_bytes_for_level_multiplier = 2;

        let db = EmbeddedDB::open(dir.path(), options).await?;
        Ok(Self {
            dir,
            db,
            values: HashMap::new(),
        })
    }

    // TODO: test that opening the same database twice causes a lock issue.

    async fn reopen(self) -> Result<Self> {
        let mut options = EmbeddedDBOptions::default();
        options.read_only = true;

        self.db.close().await?;

        let db = EmbeddedDB::open(self.dir.path(), options).await?;
        Ok(Self {
            dir: self.dir,
            db,
            values: self.values,
        })
    }

    async fn write(&mut self, write_id: u32, key_multiplier: u32, key_offset: u32) -> Result<()> {
        for i in 0..160 {
            let key_i = i * key_multiplier + key_offset;

            let key = format!("{:08}", key_i);
            let mut value = [0u8; 64 - 8];
            value[0..4].copy_from_slice(&(write_id as u32).to_be_bytes());
            value[4..8].copy_from_slice(&(key_i as u32).to_be_bytes());

            self.db.set(key.as_bytes(), &value).await?;

            self.values.remove(key.as_bytes());
            self.values.insert(key.into(), Bytes::from(&value[..]));
        }

        Ok(())
    }

    async fn verify_with_point_lookups(&self) -> Result<()> {
        for (key, value) in &self.values {
            let db_value = self.db.get(key.as_ref()).await?.unwrap();
            assert_eq!(value, &db_value);
        }

        Ok(())
    }

    async fn verify_with_scan(&self) -> Result<()> {
        let snapshot = self.db.snapshot().await;
        let mut iter = snapshot.iter().await;

        let mut num_entries = 0;
        let mut seek_keys = HashSet::new();
        while let Some(entry) = iter.next().await? {
            num_entries += 1;
            assert!(seek_keys.insert(entry.key.to_vec()), "Duplicate key seen");

            let expected_value = self.values.get(&entry.key).unwrap();

            if expected_value != &entry.value {
                iter.next().await?;
                iter.next().await?;
                iter.next().await?;
            }

            assert_eq!(expected_value, &entry.value);
        }

        assert_eq!(num_entries, self.values.len());

        Ok(())
    }
}

async fn test_table() -> Result<()> {
    let mut options = sstable::table::table::SSTableOpenOptions {
        comparator: Arc::new(BytewiseComparator::new()),
    };

    let table =
        sstable::table::table::SSTable::open(project_path!("out/big-sstable/000007.ldb"), options)
            .await?;

    let cache = DataBlockCache::new(10000000); // 10MB

    let mut iter = table.iter(&cache);

    while let Some(res) = iter.next().await? {
        println!("{:?} => {:?}", res.key, res.value);
        break;
    }

    Ok(())
}

async fn run() -> Result<()> {
    /*
    Tests to add:
    - SSTable level creation and traversal tests.
    - DB: Read a consistent snapshot while writing new keys.

    */

    // return test_table().await;

    if true {
        let mut db = TestDB::open().await?;

        /*
            64 bytes per key-value.
            - 160 kv pairs per file
        */

        // [0, 160) * 10,000
        // => Create one table into level 2
        db.write(1, 10000, 0).await?;

        db.verify_with_point_lookups().await?;

        // TODO: Replace these sleeps with explicit blocking on completion of all
        // compaction passes.
        common::async_std::task::sleep(std::time::Duration::from_secs(1)).await;

        // [0, 160] * 20,000
        // => Create one table into level 1
        db.write(2, 20000, 0).await?;

        common::async_std::task::sleep(std::time::Duration::from_secs(1)).await;

        // [0, 160] * 5,000
        // => Create one table into level 0
        db.write(3, 5000, 0).await?;

        db.verify_with_point_lookups().await?;

        common::async_std::task::sleep(std::time::Duration::from_secs(1)).await;

        // [0, 160] * 5,000 + 60*10,000
        // => Create one table into level 0
        // => Will trigger compaction with level 1
        // => Level 1 should now contain ~3 files.
        db.write(4, 5000, 60 * 10000).await?;

        println!("Done waiting!");

        common::async_std::task::sleep(std::time::Duration::from_secs(3)).await;

        db.verify_with_scan().await?;

        db = db.reopen().await?;

        db.verify_with_point_lookups().await?;

        // TODO: Also test with reloading the database from disk.

        // Another possible idea is to randomly stop the program and verify that
        // the set of written keys is still consistent and that the db is
        // recoverable.

        /*
            What are we testing:
            - That mergin
        */
    } else if false {
        let mut options = EmbeddedDBOptions::default();
        options.create_if_missing = true;
        options.error_if_exists = true;
        options.write_buffer_size = 50 * 1024;
        options.table_options.compression = CompressionType::ZLib;

        let db = EmbeddedDB::open(&project_path!("out/test-sstable"), options).await?;

        for i in 1000..10000 {
            let key = i.to_string();
            db.set(key.as_bytes(), if i % 2 == 0 { b"even" } else { b"odd" })
                .await?;
        }

        println!("Done waiting!");

        common::async_std::task::sleep(std::time::Duration::from_secs(5)).await;

        return Ok(());
    } else {
        let mut options = EmbeddedDBOptions::default();
        options.read_only = true;

        let db = EmbeddedDB::open(&project_path!("out/test-sstable/"), options).await?;

        let snapshot = db.snapshot().await;

        for i in 1000..10000 {
            let key = i.to_string();

            let mut iter = snapshot.iter().await;
            iter.seek(key.as_bytes()).await?;

            let entry = iter.next().await?.unwrap();
            assert_eq!(&entry.key, key.as_bytes(), "{:?} != {}", entry.key, key);
            assert_eq!(
                &entry.value,
                if i % 2 == 0 {
                    &b"even"[..]
                } else {
                    &b"odd"[..]
                }
            );
        }

        {
            let mut before_iter = snapshot.iter().await;
            before_iter.seek(b"0").await?;
            let entry = before_iter.next().await?.unwrap();
            assert_eq!(&entry.key[..], &b"1000"[..]);
        }

        {
            let mut after_iter = snapshot.iter().await;
            after_iter.seek(b"A").await?; // 'A' > '9'
            let entry = after_iter.next().await?;
            assert!(entry.is_none());
        }

        // Testing a full scan
        {
            let mut iter = db.snapshot().await.iter().await;

            while let Some(entry) = iter.next().await? {
                println!("{:?} => {:?}", entry.key, entry.value);
            }
        }

        // TODO: Test seeking to positions in between tables or in between keys.

        // TODO: Test point lookups.

        return Ok(());
    }

    //	sstable::table::SSTable::open(project_path!("testdata/rocksdb/000004.sst")).
    // await?;

    Ok(())
}

fn main() -> Result<()> {
    task::block_on(run())
}
