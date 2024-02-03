use std::collections::{HashMap, HashSet};

use common::bytes::Bytes;
use common::errors::*;
use common::io::Writeable;
use file::temp::TempDir;

use crate::db::write_batch::WriteBatch;
use crate::db::{Backup, Snapshot};
use crate::iterable::Iterable;
use crate::table::CompressionType;
use crate::{EmbeddedDB, EmbeddedDBOptions};

use super::SnapshotIterator;

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
        // Don't prefix compress to make the file sizes predictable for testing.
        options.table_options.block_restart_interval = 4;
        options.table_options.use_delta_encoding = false;

        options.manual_compactions_only = true;

        // Level 0 max size will be 20KB
        options.level0_file_num_compaction_trigger = 2;
        options.write_buffer_size = 10 * 1024; // 10KB

        // Make slightly larger than the write buffer to ensure that we don't create
        // tiny files with the overflow after accounting for file overhead.
        options.target_file_size_base = 12 * 1024; // 12KB
        options.target_file_size_multiplier = 1;

        // Level 1 max will be 20 KB (~2 files)
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

    /// Writes 150 key-value pairs to the database.
    ///
    /// - Each key is 8 bytes long and is a number '[0,150) * key_multipler +
    ///   key_offset
    /// - Each value is 56 bytes and is equal to [write_id, key] so specifically
    ///   unique write_ids can be used to differentiate different versions of
    ///   keys.
    /// - So each key-value will be 64 bytes + 8 for InternalKey overhead = 72
    ///   bytes.
    ///
    /// So each call to this:
    /// - Adds 10.5KiB to the memtable
    /// - Later will be 11.1KiB to SSTables (due to 4 byte restart interval
    ///   overhead per key).
    ///  
    async fn write(&mut self, write_id: u32, key_multiplier: u32, key_offset: u32) -> Result<()> {
        for i in 0..150 {
            let key_i = i * key_multiplier + key_offset;

            let key = format!("{:08}", key_i);
            assert_eq!(key.len(), 8);

            let mut value = [0u8; 64 - 8];
            value[0..4].copy_from_slice(&(write_id as u32).to_be_bytes());
            value[4..8].copy_from_slice(&(key_i as u32).to_be_bytes());

            self.db.set(key.as_bytes(), &value).await?;

            self.values.remove(key.as_bytes());
            self.values.insert(key.into(), Bytes::from(&value[..]));
        }

        Ok(())
    }

    async fn snapshot(&self) -> TestValuesSnapshot {
        let snapshot = self.db.snapshot().await;

        TestValuesSnapshot {
            snapshot,
            expected_values: self.values.clone(),
        }
    }

    async fn dir_contents(&self) -> Result<Vec<String>> {
        let mut out = vec![];

        for entry in file::read_dir(self.dir.path())? {
            out.push(entry.name().to_string());
        }

        Ok(out)
    }
}

pub struct TestValuesSnapshot {
    snapshot: Snapshot,
    expected_values: HashMap<Bytes, Bytes>,
}

impl TestValuesSnapshot {
    async fn verify_all(&self) -> Result<()> {
        self.verify_with_point_lookups().await?;
        self.verify_with_scan().await?;
        Ok(())
    }

    async fn verify_with_point_lookups(&self) -> Result<()> {
        for (key, value) in &self.expected_values {
            let db_value = self
                .snapshot
                .get(key.as_ref())
                .await?
                .ok_or_else(|| format_err!("DB missing key: {:?}", key))?;
            assert_eq!(value, &db_value);
        }

        Ok(())
    }

    async fn verify_with_scan(&self) -> Result<()> {
        let mut iter = self.snapshot.iter().await?;

        let mut num_entries = 0;
        let mut seek_keys = HashSet::new();
        while let Some(entry) = iter.next().await? {
            num_entries += 1;
            assert!(seek_keys.insert(entry.key.to_vec()), "Duplicate key seen");

            let expected_value = self.expected_values.get(&entry.key).unwrap();
            assert_eq!(Some(expected_value), entry.value.as_ref());
        }

        assert_eq!(num_entries, self.expected_values.len());

        Ok(())
    }
}

fn sets_equal(a: &[String], b: &[&str]) -> bool {
    let mut b_indices = HashSet::new();

    let mut equal = true;

    for i in 0..a.len() {
        let mut found = false;
        for j in 0..b.len() {
            if b_indices.contains(&j) {
                continue;
            }

            if a[i] == b[j] {
                b_indices.insert(j);
                found = true;
                break;
            }
        }

        if !found {
            println!("Left Only: {}", a[i]);
            equal = false;
        }
    }

    for j in 0..b.len() {
        if !b_indices.contains(&j) {
            println!("Right Only: {}", b[j]);
            equal = false;
        }
    }

    equal
}

/*
TODO: Tests to add:
- SSTable level creation and traversal tests.
- DB: Read a consistent snapshot while writing new keys.
    ^ Yes, this would be a good test.
    ^ For example, reading some values which are going to be moved to various compacted files.
- test reading from the immutable memtable.

*/

#[testcase]
async fn embedded_db_compaction_test() -> Result<()> {
    /*
    At each point here, we can test snapshots and verify files aren't deleted.
    - Even through unlink keeps files around that are still open, we may not always preserve all file descriptors if we have a lot of files.
    */

    let mut db = TestDB::open().await?;

    assert!(sets_equal(
        &db.dir_contents().await?,
        &[
            "CURRENT",
            "LOCK",
            "MANIFEST-000002",
            "IDENTITY",
            "000003.log"
        ]
    ));

    // Insert key Range: [0, 150) * 10,000
    // => Will write the memtable to one new table in level 2
    db.write(1, 10000, 0).await?;

    db.snapshot().await.verify_with_point_lookups().await?;

    db.db.wait_for_compaction().await?;

    assert!(sets_equal(
        &db.dir_contents().await?,
        &[
            "CURRENT",
            "LOCK",
            "MANIFEST-000002",
            "IDENTITY",
            // Switched to new log and created a table.
            "000004.log",
            "000005.sst"
        ]
    ));

    println!("=====");

    // TODO: Check exactly which files are present in the directory at each stage.

    let snapshot1 = db.snapshot().await;

    // Insert key range [0, 150) * 20,000
    // => Will write the memtable to one new table in level 1
    db.write(2, 20000, 0).await?;

    db.db.wait_for_compaction().await?;

    assert!(sets_equal(
        &db.dir_contents().await?,
        &[
            "CURRENT",
            "LOCK",
            "MANIFEST-000002",
            "IDENTITY",
            "000005.sst",
            // Switched to new log and created a table.
            "000006.log",
            "000007.sst"
        ]
    ));

    // Verify
    snapshot1.verify_with_point_lookups().await?;
    snapshot1.verify_with_scan().await?;
    drop(snapshot1);

    db.snapshot().await.verify_with_point_lookups().await?;

    println!("=====");

    // Insert key range [0, 150) * 5,000
    // => Will write the memtable to one new table in level 0
    db.write(3, 5000, 0).await?;

    db.snapshot().await.verify_with_point_lookups().await?;

    db.db.wait_for_compaction().await?;

    // Creating new tables 9 and new log 8.
    // At this point we have 1 tables in level 0, 1, 2, so there is no overlaps yet
    // for doing compaction.
    assert!(sets_equal(
        &db.dir_contents().await?,
        &[
            "CURRENT",
            "LOCK",
            "MANIFEST-000002",
            "IDENTITY",
            "000005.sst",
            "000007.sst",
            "000008.log",
            "000009.sst",
        ]
    ));

    let snapshot1 = db.snapshot().await;

    // Insert key range [0, 150] * 5,000 + 60*10,000
    // => Will write the memtable to one new table in level 0
    // => Because we now have 2 files in level 0, we will compact into level 1.
    db.write(4, 5000, 60 * 10000).await?;

    let snapshot2 = db.snapshot().await;

    db.db.wait_for_compaction().await?;

    db.snapshot().await.verify_all().await?;

    // Because both snapshot1 and snapshot2 are active, compactions occured but no
    // old files were removed (just the ephemeral memtable file).
    assert!(sets_equal(
        &db.dir_contents().await?,
        &[
            "CURRENT",
            "LOCK",
            "MANIFEST-000002",
            "IDENTITY",
            "000005.sst",
            "000007.sst", // Deleted
            "000009.sst", // Deleted
            "000010.log",
            // "000011.sst", // Initial flush of memtable to level 0
            "000012.sst", // Part 1/3 of compaction output
            "000013.sst", // Part 2/3 of compaction output
            "000014.sst", // Part 3/3 of compaction output
        ]
    ));

    snapshot1.verify_all().await?;
    snapshot2.verify_all().await?;

    drop(snapshot1);

    db.db.wait_for_compaction().await?;

    // No change yet because one of the snapshots is still alive.
    assert!(sets_equal(
        &db.dir_contents().await?,
        &[
            "CURRENT",
            "LOCK",
            "MANIFEST-000002",
            "IDENTITY",
            "000005.sst",
            "000007.sst", // Deleted
            "000009.sst", // Deleted
            "000010.log",
            // "000011.sst", // Initial flush of memtable to level 0
            "000012.sst", // Part 1/3 of compaction output
            "000013.sst", // Part 2/3 of compaction output
            "000014.sst", // Part 3/3 of compaction output
        ]
    ));

    drop(snapshot2);

    db.db.wait_for_compaction().await?;

    // Now the old files held by snapshot1 can be deleted
    assert!(sets_equal(
        &db.dir_contents().await?,
        &[
            "CURRENT",
            "LOCK",
            "MANIFEST-000002",
            "IDENTITY",
            "000005.sst",
            // "000007.sst", // Deleted
            // "000009.sst", // Deleted
            "000010.log",
            // "000011.sst", // Initial flush of memtable to level 0
            "000012.sst", // Part 1/3 of compaction output
            "000013.sst", // Part 2/3 of compaction output
            "000014.sst", // Part 3/3 of compaction output
        ]
    ));

    db.snapshot().await.verify_all().await?;

    db = db.reopen().await?;

    db.snapshot().await.verify_all().await?;

    /*
    // Writing to a re-opened database should still work (should write changes to
    // the log).
    {
        db.write(3, 40000, 0).await?;
        db.snapshot().await.verify_all().await?;

        db = db.reopen().await?;
        db.snapshot().await.verify_all().await?;
    }
    */

    // TODO: Another possible idea is to randomly stop the program and verify that
    // the set of written keys is still consistent and that the db is
    // recoverable.

    {
        let backup_dir = TempDir::create()?;

        let backup = db.db.backup().await?;

        let backup_path = backup_dir.path().join("backup.tar");

        let mut backup_file = file::LocalFile::open_with_options(
            &backup_path,
            &file::LocalFileOpenOptions::new()
                .create_new(true)
                .write(true),
        )?;

        backup.write_to(&mut backup_file).await?;

        backup_file.flush().await?;
        drop(backup_file);

        let mut backup_file = file::LocalFile::open(backup_path)?;

        Backup::read_from(&mut backup_file, backup_dir.path()).await?;

        db.dir = backup_dir;
        db = db.reopen().await?;
        db.snapshot().await.verify_all().await?;
    }

    Ok(())
}

#[testcase]
async fn embedded_db_large_range_test() -> Result<()> {
    let dir = TempDir::create()?;

    // Create and write to disk.
    {
        let mut options = EmbeddedDBOptions::default();
        options.create_if_missing = true;
        options.error_if_exists = true;
        options.write_buffer_size = 50 * 1024;
        options.table_options.compression = CompressionType::ZLib;

        let db = EmbeddedDB::open(dir.path(), options).await?;

        let mut batch = WriteBatch::new();

        for i in 1000..10000usize {
            let key = i.to_string();

            batch.put(key.as_bytes(), if i % 2 == 0 { b"even" } else { b"odd" });

            if batch.count() >= 100 {
                db.write(&batch).await?;
                batch.clear();
            }
        }

        if batch.count() > 0 {
            db.write(&batch).await?;
        }

        db.wait_for_compaction().await?;
    }

    {
        let mut options = EmbeddedDBOptions::default();
        options.read_only = true;

        let db = EmbeddedDB::open(dir.path(), options).await?;

        let snapshot = db.snapshot().await;

        for i in 1000..10000usize {
            let key = i.to_string();

            let mut iter = snapshot.iter().await?;
            iter.seek(key.as_bytes()).await?;

            let entry = iter.next().await?.unwrap();
            assert_eq!(&entry.key, key.as_bytes(), "{:?} != {}", entry.key, key);
            assert_eq!(
                &entry.value.as_ref().unwrap()[..],
                if i % 2 == 0 {
                    &b"even"[..]
                } else {
                    &b"odd"[..]
                }
            );
        }

        {
            let mut before_iter = snapshot.iter().await?;
            before_iter.seek(b"0").await?;
            let entry = before_iter.next().await?.unwrap();
            assert_eq!(&entry.key[..], &b"1000"[..]);
        }

        {
            let mut after_iter = snapshot.iter().await?;
            after_iter.seek(b"A").await?; // 'A' > '9'
            let entry = after_iter.next().await?;
            assert!(entry.is_none());
        }

        // Testing a full scan
        {
            let mut iter = db.snapshot().await.iter().await?;

            let mut i = 1000usize;
            while let Some(entry) = iter.next().await? {
                let key = i.to_string();
                assert_eq!(key.as_bytes(), &entry.key);

                assert_eq!(
                    &entry.value.as_ref().unwrap()[..],
                    if i % 2 == 0 {
                        &b"even"[..]
                    } else {
                        &b"odd"[..]
                    }
                );

                i += 1;
            }
        }

        // TODO: Test seeking to positions in between tables or in between keys.

        // TODO: Test point lookups.
    }

    Ok(())
}

async fn read_to_vec(path: &str) -> Result<Vec<Bytes>> {
    let mut out = vec![];

    let mut options = EmbeddedDBOptions::default();
    options.read_only = true;

    let db = EmbeddedDB::open(&project_path!(path), options).await?;
    let snapshot = db.snapshot().await;
    let mut iter = snapshot.iter().await?;

    while let Some(entry) = iter.next().await? {
        let value = match &entry.value {
            Some(v) => v,
            None => continue,
        };

        out.push(entry.key);
        out.push(value.clone());
    }

    Ok(out)
}

#[testcase]
async fn embedded_db_leveldb_compatibility_empty_test() -> Result<()> {
    let entries = read_to_vec("testdata/sstable/leveldb-empty").await?;
    assert!(entries.is_empty());
    Ok(())
}

#[testcase]
async fn embedded_db_leveldb_compatibility_food_test() -> Result<()> {
    let entries = read_to_vec("testdata/sstable/leveldb-food").await.unwrap();

    let expected: &'static [&'static [u8]] = &[
        b"apples",
        b"fruit",
        b"mozzarella",
        b"cheese",
        b"oranges",
        b"color",
        b"pizza",
        b"italy",
    ];

    assert_eq!(entries, expected);

    Ok(())
}

#[testcase]
async fn embedded_db_leveldb_compatibility_food_mutate_test() -> Result<()> {
    let entries = read_to_vec("testdata/sstable/leveldb-food-mutate").await?;

    let expected: &'static [&'static [u8]] =
        &[b"apples", b"cool", b"oranges", b"color", b"pizza", b"here"];

    assert_eq!(entries, expected);

    Ok(())
}

#[testcase]
async fn embedded_db_leveldb_compatibility_prefixed_test() -> Result<()> {
    let entries = read_to_vec("testdata/sstable/leveldb-prefixed").await?;

    let mut expected = vec![];
    for i in 10000..10100 {
        expected.push(i.to_string().as_bytes().to_vec());
        expected.push(if i % 2 == 0 {
            b"even".to_vec()
        } else {
            b"odd".to_vec()
        });
    }
    for i in 20000..20100 {
        expected.push(i.to_string().as_bytes().to_vec());
        expected.push(if i % 2 == 0 {
            b"even".to_vec()
        } else {
            b"odd".to_vec()
        });
    }
    for i in 3200..3300 {
        expected.push(i.to_string().as_bytes().to_vec());
        expected.push(if i % 2 == 0 {
            b"even".to_vec()
        } else {
            b"odd".to_vec()
        });
    }
    for i in 5000..5100 {
        expected.push(i.to_string().as_bytes().to_vec());
        expected.push(if i % 2 == 0 {
            b"even".to_vec()
        } else {
            b"odd".to_vec()
        });
    }

    assert_eq!(entries, expected);

    Ok(())
}

// TODO: We must support reading all log files in the DB directory (ignoring
// log_number in the VersionSet) for this to pass.
/*
#[testcase]
async fn embedded_db_rocksdb_compatibility_test() -> Result<()> {
    async fn check_next(iter: &mut SnapshotIterator, key: &[u8], value: &[u8]) -> Result<()> {
        let entry = iter.next().await?.unwrap();
        println!("{:?}", entry.key);
        assert_eq!(&entry.key[..], key);
        assert_eq!(&entry.value.unwrap()[..], value);
        Ok(())
    }

    let mut options = EmbeddedDBOptions::default();
    options.read_only = true;

    let db = EmbeddedDB::open(&project_path!("testdata/sstable/rocksdb"), options).await?;

    println!("Opened!");

    let snapshot = db.snapshot().await;
    let mut iter = snapshot.iter().await?;

    // TODO: Also test reverse seeking

    iter.seek(b"1573403083724-123").await?;
    check_next(&mut iter, b"1573403083724-123", b"time123").await?;
    check_next(&mut iter, b"1573403083724-124", b"time124").await?;
    check_next(&mut iter, b"1573403083724-125", b"time125").await?;
    check_next(&mut iter, b"1573403083724-126", b"time126").await?;

    // Test prefix seek.

    Ok(())
}
*/

// TODO: Test that if we swap memtables and start using a new log, we can
// recover both memtables before level 0 table for the immutable log is written
// to disk (also after it is written to disk, we should ensure we don't have
// both the log and the sstable in our version set at the same time)

// TODO: Test that we can insert into the database while traversing a snapshot.

// TODO: Test that we won't delete a SSTable while there exists a snapshot that
// references it.
// ^ yes
