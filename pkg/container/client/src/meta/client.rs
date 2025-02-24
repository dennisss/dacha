use core::ops::{Deref, DerefMut};
use std::sync::Arc;

use common::errors::*;
use datastore_meta_client::MetastoreClient;
use db_table::db::ProtobufDB;
use db_table::query_one;
use executor_multitask::impl_resource_passthrough;
use protobuf_builtins::google::protobuf::Any;
use raft_client::proto::RouteLabel;

use crate::meta::constants::ZONE_ENV_VAR;
use crate::meta::ObjectMetadataTable;
use crate::proto::ObjectMetadata;

use super::constants::META_STORE_SEEDS_ENV_VAR;

///
pub struct ClusterMetaClient {
    zone: String,
    inner: Arc<MetastoreClient>,
    db: Arc<ProtobufDB>,
}

impl_resource_passthrough!(ClusterMetaClient, inner);

impl ClusterMetaClient {
    pub async fn create(zone: &str, seeds: &[String]) -> Result<Self> {
        let mut label = RouteLabel::default();
        label.set_value(format!("{}={}", ZONE_ENV_VAR, zone));

        let inner = Arc::new(MetastoreClient::create(std::slice::from_ref(&label), seeds).await?);
        let db = Arc::new(ProtobufDB::new(inner.clone()));
        Ok(Self {
            zone: zone.to_string(),
            inner,
            db,
        })
    }

    pub async fn create_from_environment() -> Result<Self> {
        let zone = std::env::var(ZONE_ENV_VAR).map_err(|_| {
            format_err!(
                "Expected the {} environment variable to be set",
                ZONE_ENV_VAR,
            )
        })?;

        let mut seeds = std::env::var(META_STORE_SEEDS_ENV_VAR)
            .unwrap_or_default()
            .split(',')
            .map(|s| s.to_string())
            .collect::<Vec<String>>();

        seeds.retain(|s| !s.is_empty());

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

    pub fn db(&self) -> &Arc<ProtobufDB> {
        &self.db
    }

    pub async fn get_object_any(&self, name: &str) -> Result<Option<Any>> {
        let db = self.db();
        let obj = query_one!(&db, ObjectMetadataTable, "name = ?", name);

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
        let db = self.db();
        let obj = query_one!(&db, ObjectMetadataTable, "name = ?", name);

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

        self.db().insert::<ObjectMetadataTable>(&obj).await?;

        Ok(())
    }
}
