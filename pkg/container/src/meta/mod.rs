/*
What we need:
- The

Metastore schema:

/cluster/job/[job_name]: JobMetadata proto
/cluster/task/[task_name] : TaskMetadata proto
/cluster/node/[node_id] : NodeMetadata
    => Also contains list of all currently assigned resources.
    resources_reserved
    resources_limit
    resources_available

/cluster/task_by_node/[node_name]/[task_name]: ""
    => Means that when we update a task, we must look it up to remove the old assigmnet
    => Generally can be locally done if it is a proto.


*/

pub mod client;
pub mod constants;

use std::collections::HashSet;
use std::marker::PhantomData;

use common::errors::*;
use datastore::meta::client::{MetastoreClient, MetastoreClientInterface};
use protobuf::Message;

use crate::proto::meta::*;

pub struct ClusterMetaTable<'a, T: ClusterMetaTableValue> {
    typ: PhantomData<T>,
    client: &'a dyn MetastoreClientInterface,
}

impl<'a, T: ClusterMetaTableValue> ClusterMetaTable<'a, T> {
    pub async fn get(&self, id: &T::Id) -> Result<Option<T>> {
        Self::get_impl(self.client, &T::primary_key_from_id(id)).await
    }

    async fn get_impl(client: &dyn MetastoreClientInterface, key: &str) -> Result<Option<T>> {
        if let Some(data) = client.get(key.as_bytes()).await? {
            return Ok(Some(T::parse(&data)?));
        }

        Ok(None)
    }

    /// Enumerates all objects in this table.
    pub async fn list(&self) -> Result<Vec<T>> {
        let entries = self.client.list(T::primary_key_prefix().as_bytes()).await?;

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
                    txn.put(new_key.as_bytes(), b"").await?;
                }
            }

            for old_key in old_secondary_keys {
                txn.delete(old_key.as_bytes()).await?;
            }

            txn.put(key.as_bytes(), &value.serialize()?).await?;

            txn.commit().await?;
            return Ok(());
        }

        self.client.put(key.as_bytes(), &value.serialize()?).await
    }

    pub async fn delete(&self, value: &T) -> Result<()> {
        let key = value.primary_key();

        if T::supports_secondary_keys() {
            let txn = self.client.new_transaction().await?;
            if let Some(old_value) = Self::get_impl(&txn, &key).await? {
                for key in old_value.secondary_keys() {
                    txn.delete(key.as_bytes()).await?;
                }
            }
            txn.delete(key.as_bytes()).await?;
            txn.commit().await?;
            return Ok(());
        }

        self.client.delete(key.as_bytes()).await
    }

    // pub async fn list_prefix()
}

impl<'a, T: ClusterMetaTableValue<Id = str>> ClusterMetaTable<'a, T> {
    /// Enumerates all values whose key starts with the given prefix.
    pub async fn get_prefix(&self, prefix: &str) -> Result<Vec<T>> {
        let entries = self
            .client
            .get_prefix(T::primary_key_from_id(prefix).as_bytes())
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

pub trait ClusterMetaTableValue: Message {
    type Id: ?Sized;

    /// Retrieves the row key for the current value.
    fn primary_key(&self) -> String;

    /// Gets the prefix used to store all values in the same table.
    fn primary_key_prefix() -> &'static str;

    fn primary_key_from_id(id: &Self::Id) -> String;

    /// NOTE: All secondary keys must be unique
    fn secondary_keys(&self) -> Vec<String> {
        vec![]
    }

    fn supports_secondary_keys() -> bool {
        false
    }
}

impl ClusterMetaTableValue for JobMetadata {
    type Id = str;

    fn primary_key(&self) -> String {
        Self::primary_key_from_id(self.spec().name())
    }

    fn primary_key_prefix() -> &'static str {
        "/cluster/job"
    }

    fn primary_key_from_id(id: &Self::Id) -> String {
        format!("{}/{}", Self::primary_key_prefix(), id)
    }
}

impl ClusterMetaTableValue for TaskMetadata {
    type Id = str;

    fn primary_key(&self) -> String {
        Self::primary_key_from_id(self.spec().name())
    }

    fn primary_key_prefix() -> &'static str {
        "/cluster/task"
    }

    fn primary_key_from_id(id: &Self::Id) -> String {
        format!("{}/{}", Self::primary_key_prefix(), id)
    }

    fn secondary_keys(&self) -> Vec<String> {
        vec![format!(
            "/cluster/task_by_node/{:08x}/{}",
            self.assigned_node(),
            self.spec().name()
        )]
    }

    fn supports_secondary_keys() -> bool {
        true
    }
}

impl<'a> ClusterMetaTable<'a, TaskMetadata> {
    pub async fn list_by_node(&self, node_id: u64) -> Result<Vec<TaskMetadata>> {
        let prefix = format!("/cluster/task_by_node/{:08x}/", node_id);

        let entries = self.client.get_prefix(prefix.as_bytes()).await?;

        let mut tasks = vec![];
        for entry in entries {
            let task_name = entry.key().strip_prefix(prefix.as_bytes()).ok_or_else(|| {
                format_err!(
                    "Invalid index key: {}",
                    std::str::from_utf8(entry.key()).unwrap()
                )
            })?;
            let task_name = std::str::from_utf8(task_name)?;

            tasks.push(
                self.get(task_name)
                    .await?
                    .ok_or_else(|| format_err!("Missing indexed value: {}", task_name))?,
            );
        }

        Ok(tasks)
    }
}

impl ClusterMetaTableValue for TaskStateMetadata {
    type Id = str;

    fn primary_key(&self) -> String {
        Self::primary_key_from_id(self.task_name())
    }

    fn primary_key_prefix() -> &'static str {
        "/cluster/task_state"
    }

    fn primary_key_from_id(id: &Self::Id) -> String {
        format!("{}/{}", Self::primary_key_prefix(), id)
    }
}

impl ClusterMetaTableValue for BlobMetadata {
    type Id = str;

    fn primary_key(&self) -> String {
        Self::primary_key_from_id(self.spec().id())
    }

    fn primary_key_prefix() -> &'static str {
        "/cluster/blob"
    }

    fn primary_key_from_id(id: &Self::Id) -> String {
        format!("{}/{}", Self::primary_key_prefix(), id)
    }
}

impl ClusterMetaTableValue for NodeMetadata {
    type Id = u64;

    fn primary_key(&self) -> String {
        Self::primary_key_from_id(&self.id())
    }

    fn primary_key_prefix() -> &'static str {
        "/cluster/node"
    }

    fn primary_key_from_id(id: &Self::Id) -> String {
        format!("{}/{:08x}", Self::primary_key_prefix(), id)
    }
}

impl ClusterMetaTableValue for ZoneMetadata {
    type Id = ();

    fn primary_key(&self) -> String {
        Self::primary_key_from_id(&())
    }

    fn primary_key_prefix() -> &'static str {
        "/cluster/zone"
    }

    fn primary_key_from_id(id: &Self::Id) -> String {
        Self::primary_key_prefix().to_string()
    }
}
