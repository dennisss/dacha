use std::collections::HashMap;

use base_error::*;
use common::bytes::Bytes;
use db_kv::*;
use db_table::db::*;
use db_table::key::*;
use db_table::query::*;
use db_table::table::*;
use executor::lock;
use executor::sync::AsyncMutex;
use file::LocalPath;
use protobuf::reflection::{Reflection, ReflectionMut};
use protobuf::{FieldNumber, Message, MessageReflection, StaticMessage, TypedFieldNumber};
use sstable::db::SnapshotIterator;
use sstable::db::WriteBatch;
use sstable::{db::SnapshotIteratorOptions, iterable::Iterable};
use sstable::{EmbeddedDB, EmbeddedDBOptions};

pub async fn create_db_instance(path: &LocalPath) -> Result<ProtobufDB> {
    let mut options = EmbeddedDBOptions::default();
    options.create_if_missing = true;
    options.error_if_exists = false;

    let db = EmbeddedDB::open(path, options).await?;

    Ok(ProtobufDB::new(Box::new(EmbeddedDBWrapper { db })))
}

struct EmbeddedDBWrapper {
    db: EmbeddedDB,
}

#[async_trait]
impl KeyValueStore for EmbeddedDBWrapper {
    async fn new_transaction<'a>(&'a self) -> Result<Box<dyn KeyValueStoreTransaction + 'a>> {
        Ok(Box::new(SimpleTransaction {
            db: self,
            write: AsyncMutex::new(WriteBatch::new()),
        }))
    }
}

// TODO: Currently this doesn't provide full transactional guarantees (we don't
// verify that read rows don't change).
struct SimpleTransaction<'a> {
    db: &'a EmbeddedDBWrapper,
    write: AsyncMutex<WriteBatch>,
}

#[async_trait]
impl<'a> KeyValueStoreTransaction for SimpleTransaction<'a> {
    async fn iter(
        &self,
        options: KeyValueIteratorOptions,
    ) -> Result<Box<dyn KeyValueStoreIterator>> {
        // TODO: Maintain a single snapshot for the whole transaction.
        let mut snapshot = self.db.db.snapshot().await;

        let mut iter = snapshot.iter().await?;
        iter.seek(&options.start_key).await?;

        Ok(Box::new(SimpleIterator {
            iter,
            max_key: options.end_key,
        }))
    }

    async fn put(&self, key: &[u8], value: &[u8]) -> Result<()> {
        lock!(write <= self.write.lock().await?, {
            write.put(key, value);
        });

        Ok(())
    }

    // TODO: To make this useful, we need to start aggresively validating that there
    // is no duplication in the WriteBatch.
    async fn delete(&self, key: &[u8]) -> Result<()> {
        lock!(write <= self.write.lock().await?, {
            write.delete(key);
        });

        Ok(())
    }

    async fn commit(&self) -> Result<()> {
        // TODO: Verify we are never comitting twice.
        let write = self.write.lock().await?.read_exclusive();
        self.db.db.write(&write).await
    }
}

struct SimpleIterator {
    iter: SnapshotIterator,
    max_key: Bytes,
}

#[async_trait]
impl KeyValueStoreIterator for SimpleIterator {
    async fn next(&mut self) -> Result<Option<KeyValueEntry>> {
        while let Some(entry) = self.iter.next().await? {
            if &entry.key >= &self.max_key {
                return Ok(None);
            }

            let value = match entry.value {
                Some(v) => v,
                None => continue,
            };

            return Ok(Some(KeyValueEntry {
                key: entry.key,
                value,
            }));
        }

        Ok(None)
    }
}

// TODO: Move to the db_table crate.
struct TableIndexIterator<Tag: ProtobufTableTag + 'static> {
    key_config: &'static ProtobufTableKey<Tag>,
    min_key: Vec<u8>,
    max_key: Vec<u8>,
}

impl<Tag: ProtobufTableTag + 'static> TableIndexIterator<Tag> {
    async fn next(&self) -> Result<Option<Tag::Message>> {
        todo!()
    }
}
