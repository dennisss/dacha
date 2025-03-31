use base_error::*;
use common::bytes::Bytes;
use db_kv::*;
use sstable::{
    db::{Snapshot, SnapshotIterator, SnapshotIteratorOptions},
    iterable::Iterable,
};

pub struct View {
    snapshot: Snapshot,
}

impl View {
    pub fn new(snapshot: Snapshot) -> Self {
        Self { snapshot }
    }
}

#[async_trait]
impl KeyValueStore for View {
    async fn new_transaction<'a>(&'a self) -> Result<Box<dyn KeyValueStoreTransaction + 'a>> {
        Ok(Box::new(ViewTransaction { inst: self }))
    }
}

struct ViewTransaction<'a> {
    inst: &'a View,
}

#[async_trait]
impl<'a> KeyValueStoreTransaction for ViewTransaction<'a> {
    async fn iter<'b>(
        &'b self,
        options: KeyValueIteratorOptions,
    ) -> Result<Box<dyn KeyValueStoreIterator + 'b>> {
        let mut iter = self.inst.snapshot.iter().await?;
        iter.seek(&options.start_key).await?;

        Ok(Box::new(Iterator {
            iter,
            max_key: options.end_key,
        }))
    }

    async fn read_index(&self) -> u64 {
        todo!()
    }

    async fn put(&mut self, key: &[u8], value: &[u8]) -> Result<()> {
        Err(err_msg("Read only view of the db"))
    }

    // TODO: To make this useful, we need to start aggresively validating that there
    // is no duplication in the WriteBatch.
    async fn delete(&mut self, key: &[u8]) -> Result<()> {
        Err(err_msg("Read only view of the db"))
    }

    async fn commit(&mut self) -> Result<()> {
        Ok(())
    }
}

struct Iterator {
    iter: SnapshotIterator,

    max_key: Bytes,
}

#[async_trait]
impl KeyValueStoreIterator for Iterator {
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
