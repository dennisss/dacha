use std::ops::{Deref, DerefMut};

use common::errors::*;
use datastore::meta::client::MetastoreClient;
use raft::proto::routing::RouteLabel;

use crate::meta::constants::ZONE_ENV_VAR;

pub struct ClusterMetaClient {
    zone: String,
    inner: MetastoreClient,
}

impl ClusterMetaClient {
    pub async fn create(zone: &str) -> Result<Self> {
        let mut label = RouteLabel::default();
        label.set_value(format!("{}={}", ZONE_ENV_VAR, zone));

        let inner = MetastoreClient::create(std::slice::from_ref(&label)).await?;
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
        Self::create(&zone).await
    }

    pub fn zone(&self) -> &str {
        &self.zone
    }

    pub fn inner(&self) -> &MetastoreClient {
        &self.inner
    }
}

#[async_trait]
impl datastore::meta::client::MetastoreClientInterface for ClusterMetaClient {
    async fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>> {
        self.inner.get(key).await
    }

    async fn get_range(
        &self,
        start_key: &[u8],
        end_key: &[u8],
    ) -> Result<Vec<datastore::proto::key_value::KeyValueEntry>> {
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
    ) -> Result<datastore::meta::client::MetastoreTransaction<'a>> {
        self.inner.new_transaction().await
    }
}
