/*
What we need:
- The

Metastore schema:

/cluster/job/[job_name]: JobMetadata proto
/cluster/worker/[worker_name] : WorkerMetadata proto
/cluster/node/[node_id] : NodeMetadata
    => Also contains list of all currently assigned resources.
    resources_reserved
    resources_limit
    resources_available

/cluster/worker_by_node/[node_name]/[worker_name]: ""
    => Means that when we update a worker, we must look it up to remove the old assigmnet
    => Generally can be locally done if it is a proto.

*/

pub mod client;
pub mod constants;

use std::collections::HashSet;
use std::marker::PhantomData;

use common::errors::*;
use datastore::key_encoding::KeyEncoder;
use datastore::meta::client::{MetastoreClient, MetastoreClientInterface};
use protobuf::{Enum, Message, StaticMessage};

use crate::proto::meta::*;

pub struct ClusterMetaTable<'a, T: ClusterMetaTableValue> {
    typ: PhantomData<T>,
    client: &'a dyn MetastoreClientInterface,
}

impl<'a, T: ClusterMetaTableValue> ClusterMetaTable<'a, T> {
    pub async fn get(&self, id: &T::Id) -> Result<Option<T>> {
        Self::get_impl(self.client, &T::primary_key_from_id(id)).await
    }

    async fn get_impl(client: &dyn MetastoreClientInterface, key: &[u8]) -> Result<Option<T>> {
        if let Some(data) = client.get(key).await? {
            return Ok(Some(T::parse(&data)?));
        }

        Ok(None)
    }

    /// Enumerates all objects in this table.
    pub async fn list(&self) -> Result<Vec<T>> {
        let entries = self.client.get_prefix(&T::primary_key_prefix()).await?;

        let mut out = vec![];
        for entry in entries {
            out.push(T::parse(entry.value())?);
        }

        Ok(out)
    }

    pub async fn put(&self, value: &T) -> Result<()> {
        let key = value.primary_key();

        if T::supports_secondary_keys() {
            let txn = self.client.new_transaction().await?;

            let mut old_secondary_keys = HashSet::new();
            if let Some(old_value) = Self::get_impl(&txn, &key).await? {
                for key in old_value.secondary_keys() {
                    old_secondary_keys.insert(key);
                }
            }

            for new_key in value.secondary_keys() {
                if !old_secondary_keys.remove(&new_key) {
                    txn.put(&new_key, b"").await?;
                }
            }

            for old_key in old_secondary_keys {
                txn.delete(&old_key).await?;
            }

            txn.put(&key, &value.serialize()?).await?;

            txn.commit().await?;
            return Ok(());
        }

        self.client.put(&key, &value.serialize()?).await
    }

    pub async fn delete(&self, value: &T) -> Result<()> {
        let key = value.primary_key();

        if T::supports_secondary_keys() {
            let txn = self.client.new_transaction().await?;
            if let Some(old_value) = Self::get_impl(&txn, &key).await? {
                for key in old_value.secondary_keys() {
                    txn.delete(&key).await?;
                }
            }
            txn.delete(&key).await?;
            txn.commit().await?;
            return Ok(());
        }

        self.client.delete(&key).await
    }

    // pub async fn list_prefix()
}

impl<'a, T: ClusterMetaTableValue<Id = str>> ClusterMetaTable<'a, T> {
    /// Enumerates all values whose key starts with the given prefix.
    pub async fn get_prefix(&self, prefix: &str) -> Result<Vec<T>> {
        // NOTE: This only works because we use KeyEncoder::encode_end_bytes
        // (KeyEncoder::encode_bytes doesn't work with prefixes).
        let entries = self
            .client
            .get_prefix(&T::primary_key_from_id(prefix))
            .await?;

        let mut out = vec![];
        for entry in entries {
            out.push(T::parse(entry.value())?);
        }

        Ok(out)
    }
}

pub trait GetClusterMetaTable {
    fn cluster_table<'a, T: ClusterMetaTableValue>(&'a self) -> ClusterMetaTable<'a, T>;
}

impl GetClusterMetaTable for dyn MetastoreClientInterface {
    fn cluster_table<T: ClusterMetaTableValue>(&self) -> ClusterMetaTable<T> {
        ClusterMetaTable {
            typ: PhantomData,
            client: self,
        }
    }
}

impl<C: MetastoreClientInterface> GetClusterMetaTable for C {
    fn cluster_table<T: ClusterMetaTableValue>(&self) -> ClusterMetaTable<T> {
        ClusterMetaTable {
            typ: PhantomData,
            client: self,
        }
    }
}

pub trait ClusterMetaTableValue: StaticMessage {
    type Id: ?Sized;

    /// Retrieves the row key for the current value.
    fn primary_key(&self) -> Vec<u8>;

    fn primary_table_id() -> ClusterTableId;

    /// Gets the prefix used to store all values in the same table.
    fn primary_key_prefix() -> Vec<u8> {
        let mut out = vec![];
        KeyEncoder::encode_varuint(Self::primary_table_id().value() as u64, false, &mut out);
        out
    }

    fn primary_key_from_id(id: &Self::Id) -> Vec<u8>;

    /// NOTE: All secondary keys must be unique
    fn secondary_keys(&self) -> Vec<Vec<u8>> {
        vec![]
    }

    fn supports_secondary_keys() -> bool {
        false
    }
}

impl ClusterMetaTableValue for JobMetadata {
    type Id = str;

    fn primary_key(&self) -> Vec<u8> {
        Self::primary_key_from_id(self.spec().name())
    }

    fn primary_table_id() -> ClusterTableId {
        ClusterTableId::Job
    }

    fn primary_key_from_id(id: &Self::Id) -> Vec<u8> {
        (Self::primary_table_id(), id).to_table_key()
    }
}

impl ClusterMetaTableValue for WorkerMetadata {
    type Id = str;

    fn primary_key(&self) -> Vec<u8> {
        Self::primary_key_from_id(self.spec().name())
    }

    fn primary_table_id() -> ClusterTableId {
        ClusterTableId::Worker
    }

    fn primary_key_from_id(id: &Self::Id) -> Vec<u8> {
        (Self::primary_table_id(), id).to_table_key()
    }

    fn secondary_keys(&self) -> Vec<Vec<u8>> {
        vec![(
            ClusterTableId::WorkerByNode,
            self.assigned_node(),
            self.spec().name(),
        )
            .to_table_key()]
    }

    fn supports_secondary_keys() -> bool {
        true
    }
}

impl<'a> ClusterMetaTable<'a, WorkerMetadata> {
    pub async fn list_by_job(&self, job_name: &str) -> Result<Vec<WorkerMetadata>> {
        self.get_prefix(&format!("{}.", job_name)).await
    }

    pub async fn list_by_node(&self, node_id: u64) -> Result<Vec<WorkerMetadata>> {
        let prefix = (ClusterTableId::WorkerByNode, node_id).to_table_key();

        let entries = self.client.get_prefix(&prefix).await?;

        let mut workers = vec![];
        for entry in entries {
            let key_suffix = entry
                .key()
                .strip_prefix(&prefix[..])
                .ok_or_else(|| err_msg("Invalid index key"))?;

            let (worker_name_bytes, rest) = KeyEncoder::decode_end_bytes(key_suffix)?;
            if !rest.is_empty() {
                return Err(err_msg("Extra bytes after worker_name"));
            }

            let worker_name = std::str::from_utf8(worker_name_bytes)?;

            workers.push(
                self.get(worker_name)
                    .await?
                    .ok_or_else(|| format_err!("Missing indexed value: {}", worker_name))?,
            );
        }

        Ok(workers)
    }
}

impl ClusterMetaTableValue for WorkerStateMetadata {
    type Id = str;

    fn primary_key(&self) -> Vec<u8> {
        Self::primary_key_from_id(self.worker_name())
    }

    fn primary_table_id() -> ClusterTableId {
        ClusterTableId::WorkerState
    }

    fn primary_key_from_id(id: &Self::Id) -> Vec<u8> {
        (Self::primary_table_id(), id).to_table_key()
    }
}

impl ClusterMetaTableValue for BlobMetadata {
    type Id = str;

    fn primary_key(&self) -> Vec<u8> {
        Self::primary_key_from_id(self.spec().id())
    }

    fn primary_table_id() -> ClusterTableId {
        ClusterTableId::Blob
    }

    fn primary_key_from_id(id: &Self::Id) -> Vec<u8> {
        (Self::primary_table_id(), id).to_table_key()
    }
}

impl ClusterMetaTableValue for NodeMetadata {
    type Id = u64;

    fn primary_key(&self) -> Vec<u8> {
        Self::primary_key_from_id(&self.id())
    }

    fn primary_table_id() -> ClusterTableId {
        ClusterTableId::Node
    }

    fn primary_key_from_id(id: &Self::Id) -> Vec<u8> {
        (Self::primary_table_id(), *id).to_table_key()
    }
}

impl ClusterMetaTableValue for ZoneMetadata {
    type Id = ();

    fn primary_key(&self) -> Vec<u8> {
        Self::primary_key_from_id(&())
    }

    fn primary_table_id() -> ClusterTableId {
        ClusterTableId::Zone
    }

    fn primary_key_from_id(id: &Self::Id) -> Vec<u8> {
        Self::primary_table_id().to_table_key()
    }
}

impl ClusterMetaTableValue for ObjectMetadata {
    type Id = str;

    fn primary_key(&self) -> Vec<u8> {
        Self::primary_key_from_id(self.name())
    }

    fn primary_table_id() -> ClusterTableId {
        ClusterTableId::Object
    }

    fn primary_key_from_id(id: &Self::Id) -> Vec<u8> {
        (Self::primary_table_id(), id).to_table_key()
    }
}

trait ToTableKey {
    fn to_table_key(&self) -> Vec<u8>;
}

impl ToTableKey for ClusterTableId {
    fn to_table_key(&self) -> Vec<u8> {
        let mut out = vec![];
        KeyEncoder::encode_varuint(self.value() as u64, false, &mut out);
        out
    }
}

impl<'a> ToTableKey for (ClusterTableId, &'a str) {
    fn to_table_key(&self) -> Vec<u8> {
        let mut out = vec![];
        KeyEncoder::encode_varuint(self.0.value() as u64, false, &mut out);
        KeyEncoder::encode_end_bytes(self.1.as_bytes(), &mut out);
        out
    }
}

impl ToTableKey for (ClusterTableId, u64) {
    fn to_table_key(&self) -> Vec<u8> {
        let mut out = vec![];
        KeyEncoder::encode_varuint(self.0.value() as u64, false, &mut out);
        KeyEncoder::encode_varuint(self.1, false, &mut out);
        out
    }
}

impl<'a> ToTableKey for (ClusterTableId, u64, &'a str) {
    fn to_table_key(&self) -> Vec<u8> {
        let mut out = vec![];
        KeyEncoder::encode_varuint(self.0.value() as u64, false, &mut out);
        KeyEncoder::encode_varuint(self.1, false, &mut out);
        KeyEncoder::encode_end_bytes(self.2.as_bytes(), &mut out);
        out
    }
}
