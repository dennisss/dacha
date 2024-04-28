use core::ops::{Deref, DerefMut};

use common::errors::*;
use datastore_meta_client::MetastoreClient;
use datastore_meta_client::MetastoreClientInterface;
use executor_multitask::impl_resource_passthrough;
use protobuf_builtins::google::protobuf::Any;
use raft_client::proto::RouteLabel;

use crate::meta::constants::ZONE_ENV_VAR;
use crate::meta::{ClusterMetaTable, GetClusterMetaTable};
use crate::proto::ObjectMetadata;

use super::constants::META_STORE_SEEDS_ENV_VAR;

///
pub struct ClusterMetaClient {
    zone: String,
    inner: MetastoreClient,
}

impl_resource_passthrough!(ClusterMetaClient, inner);

impl ClusterMetaClient {
    pub async fn create(zone: &str, seeds: &[String]) -> Result<Self> {
        let mut label = RouteLabel::default();
        label.set_value(format!("{}={}", ZONE_ENV_VAR, zone));

        let inner = MetastoreClient::create(std::slice::from_ref(&label), seeds).await?;
        Ok(Self {
            zone: zone.to_string(),
            inner,
        })
    }

    pub async fn create_from_environment() -> Result<Self> {
        let zone = std::env::var(ZONE_ENV_VAR).map_err(|_| {
            format_err!(
                "Expected the {} environment variable to be set",
                ZONE_ENV_VAR,
            )
        })?;

        let seeds = std::env::var(META_STORE_SEEDS_ENV_VAR)
            .unwrap_or_default()
            .split(',')
            .map(|s| s.to_string())
            .collect::<Vec<String>>();

        if seeds.is_empty() {
            eprintln!(
                "WARN: {} env var empty. Must fallback to multicast discovery.",
                META_STORE_SEEDS_ENV_VAR
            )
        }

        Self::create(&zone, &seeds).await
    }

    pub fn zone(&self) -> &str {
        &self.zone
    }

    pub fn inner(&self) -> &MetastoreClient {
        &self.inner
    }

    pub async fn get_object_any(&self, name: &str) -> Result<Option<Any>> {
        let obj = self
            .inner
            .cluster_table::<ObjectMetadata>()
            .get(name)
            .await?;
        if let Some(obj) = obj {
            Ok(Some(obj.value().clone()))
        } else {
            Ok(None)
        }
    }

    pub async fn get_object<M: protobuf::Message + Default>(
        &self,
        name: &str,
    ) -> Result<Option<M>> {
        let obj = self
            .inner
            .cluster_table::<ObjectMetadata>()
            .get(name)
            .await?;
        if let Some(obj) = obj {
            Ok(Some(
                obj.value()
                    .unpack()?
                    .ok_or_else(|| err_msg("Object configs different type"))?,
            ))
        } else {
            Ok(None)
        }
    }

    pub async fn set_object<M: protobuf::Message>(&self, name: &str, value: &M) -> Result<()> {
        let mut obj = ObjectMetadata::default();
        obj.set_name(name);
        obj.value_mut().pack_from(value)?;

        self.inner
            .cluster_table::<ObjectMetadata>()
            .put(&obj)
            .await?;
        Ok(())
    }
}

#[async_trait]
impl MetastoreClientInterface for ClusterMetaClient {
    async fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>> {
        self.inner.get(key).await
    }

    async fn get_range(
        &self,
        start_key: &[u8],
        end_key: &[u8],
    ) -> Result<Vec<datastore_proto::db::meta::KeyValueEntry>> {
        self.inner.get_range(start_key, end_key).await
    }

    async fn put(&self, key: &[u8], value: &[u8]) -> Result<()> {
        self.inner.put(key, value).await
    }

    async fn delete(&self, key: &[u8]) -> Result<()> {
        self.inner.delete(key).await
    }

    async fn new_transaction<'a>(
        &'a self,
    ) -> Result<datastore_meta_client::MetastoreTransaction<'a>> {
        self.inner.new_transaction().await
    }
}
