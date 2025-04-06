use base_error::*;
use common::bytes::Bytes;

//// Interface for interacting with a raw database that is accessed like a map
//// of ordered keys with binary blob values.
///
/// This is specifically for one that supports read-modify-write style
/// transactions.
#[async_trait]
pub trait KeyValueStore: Send + Sync {
    async fn new_transaction<'a>(&'a self) -> Result<Box<dyn KeyValueStoreTransaction + 'a>>;
}

/// An atomic sequence of read/write operations that are only applied to the
/// underlying database once it is commited.
///
/// Within a single transaction, reads will factor in previous writes to the
/// transaction and the implementation should be flexible to keys being
/// potentially read/written multiple times sequentially.
///
/// Note that parallel reads and writes are not supported.
#[async_trait]
pub trait KeyValueStoreTransaction: Send + Sync {
    // Looks up a single key in the database.
    //
    // Implementers may just implement this using iter() or provide a more optimized version.
    async fn get(&self, key: &[u8]) -> Result<Option<Bytes>>;

    // TODO: Need an optimized version for single key lookups?
    //
    // TODO: Ensure this doesn't lock the full range if we don't end up iterating
    // over the full range.
    async fn iter<'a>(
        &'a self,
        options: KeyValueIteratorOptions,
    ) -> Result<Box<dyn KeyValueStoreIterator + 'a>>;

    async fn read_index(&self) -> u64;

    /// Either inserts a new key-value pair or updates the existing one.
    async fn put(&mut self, key: &[u8], value: &[u8]) -> Result<()>;

    /// NOTE: This shouldn't check if the key actually exists (it may succeed
    /// with just leaving a deletion tombstone).
    async fn delete(&mut self, key: &[u8]) -> Result<()>;

    /// NOTE: Attempting to call this function more than once should error out.
    async fn commit(&mut self) -> Result<()>;
}

#[derive(Debug)]
pub struct KeyValueIteratorOptions {
    pub start_key: Bytes,

    // TODO: Support Option<Bytes> to supprot going all the way to the end.
    pub end_key: Bytes,
}

// TODO: Maybe dedup with 'trait Iterable'
#[async_trait]
pub trait KeyValueStoreIterator: Send {
    async fn next(&mut self) -> Result<Option<KeyValueEntry>>;
}

#[derive(Clone, Debug, PartialEq)]
pub struct KeyValueEntry {
    pub key: Bytes,
    pub value: Bytes,
}
