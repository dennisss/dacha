use base_error::*;
use common::bytes::Bytes;

//// Interface for interacting with a raw database that is accessed like a map
//// of ordered keys with binary blob values.
///
/// This is specifically for one that supports read-modify-write style
/// transactions.
#[async_trait]
pub trait KeyValueStore: Send + Sync {
    /*
    /// Looks up a single value from the database.
    async fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>>;

    /// Looks
    async fn get_range(&self, start_key: &[u8], end_key: &[u8]) -> Result<Vec<KeyValueEntry>>;

    async fn get_prefix(&self, prefix: &[u8]) -> Result<Vec<KeyValueEntry>> {
        let (start_key, end_key) = prefix_key_range(prefix);
        self.get_range(&start_key, &end_key).await
    }

    async fn put(&self, key: &[u8], value: &[u8]) -> Result<()>;

    async fn delete(&self, key: &[u8]) -> Result<()>;
    */

    async fn new_transaction<'a>(&'a self) -> Result<Box<dyn KeyValueStoreTransaction + 'a>>;
}

#[async_trait]
pub trait KeyValueStoreTransaction: Send + Sync {
    // TODO: Need an optimized version for single key lookups?
    async fn iter(
        &self,
        options: KeyValueIteratorOptions,
    ) -> Result<Box<dyn KeyValueStoreIterator>>;

    async fn put(&self, key: &[u8], value: &[u8]) -> Result<()>;

    /// NOTE: This shouldn't check if the key actually exists (it may suceed
    /// with just leaving a deletion tombstone).
    async fn delete(&self, key: &[u8]) -> Result<()>;

    /// NOTE: Attempting to call this function more than once should error out.
    async fn commit(&self) -> Result<()>;
}

pub struct KeyValueIteratorOptions {
    pub start_key: Bytes,
    pub end_key: Bytes,
}

// TODO: Maybe dedup with 'trait Iterable'
#[async_trait]
pub trait KeyValueStoreIterator: Send {
    async fn next(&mut self) -> Result<Option<KeyValueEntry>>;
}

#[derive(Clone, Debug)]
pub struct KeyValueEntry {
    pub key: Bytes,
    pub value: Bytes,
}
