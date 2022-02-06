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
