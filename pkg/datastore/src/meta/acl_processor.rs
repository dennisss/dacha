use common::errors::*;
use datastore_proto::db::meta::*;
use sstable::db::{Snapshot, WriteBatch};

/// Hooks called before reads/writes on the metastore to enforce
/// implementation/data specific ACLs.
#[async_trait]
pub trait ACLProcessor: Send + Sync {
    /// Called when a client attempts to read some data via RPC.
    ///
    /// - 'snapshot' has the latest data in the database though the client may
    ///   be attempting to read from a slightly older position.
    ///
    /// If this fails, then the RPC will error out before the read is performed.
    async fn before_read(
        &self,
        snapshot: &Snapshot,
        key_ranges: &[KeyRange],
        context: &rpc::ServerRequestContext,
    ) -> Result<()>;

    /// Called before a client attempts to execute a transaction via RPC.
    ///
    /// If this fails, then the RPC will error out before the read is performed.
    async fn before_execute(
        &self,
        snapshot: &Snapshot,
        transaction: &Transaction,
        context: &rpc::ServerRequestContext,
    ) -> Result<()>;
}
