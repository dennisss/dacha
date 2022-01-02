use std::ops::{Deref, DerefMut};

use common::errors::*;
use datastore::meta::client::MetastoreClient;
use raft::proto::routing::RouteLabel;

use crate::meta::constants::ZONE_ENV_VAR;

pub struct ClusterMetaClient {
    inner: MetastoreClient,
}

impl ClusterMetaClient {
    pub async fn create(zone: &str) -> Result<Self> {
        let mut label = RouteLabel::default();
        label.set_value(format!("{}={}", ZONE_ENV_VAR, zone));

        let inner = MetastoreClient::create(std::slice::from_ref(&label)).await?;
        Ok(Self { inner })
    }

    pub async fn create_from_environment() -> Result<Self> {
        let zone = std::env::var(ZONE_ENV_VAR)?;
        Self::create(&zone).await
    }
}

impl Deref for ClusterMetaClient {
    type Target = MetastoreClient;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl DerefMut for ClusterMetaClient {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}
