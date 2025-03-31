use core::ops::{Deref, DerefMut};
use std::sync::Arc;

use common::errors::*;
use container_proto::cluster::ObjectMetadata;
use datastore_meta_client::MetastoreClient;
use db_table::db::ProtobufDB;
use db_table::query_one;
use executor_multitask::impl_resource_passthrough;
use protobuf::{Message, StaticMessage};
use protobuf_builtins::google::protobuf::Any;
use raft_client::proto::RouteLabel;

use crate::credentials::get_cluster_credentials;
use crate::env::ZONE_ENV_VAR;
use crate::meta::ObjectMetadataTable;

use super::constants::META_STORE_SEEDS_ENV_VAR;
use super::hostname::ClusterMetaHostnameResolver;

///
pub struct ClusterMetaClient {
    zone: String,
    inner: Arc<MetastoreClient>,
    db: Arc<ProtobufDB>,
    creds: Option<crypto::tls::Credentials>,
}

impl_resource_passthrough!(ClusterMetaClient, inner);

impl ClusterMetaClient {
    pub async fn create(
        zone: &str,
        seeds: &[String],
        creds: Option<crypto::tls::Credentials>,
    ) -> Result<Self> {
        let mut label = RouteLabel::default();
        label.set_value(format!("{}={}", ZONE_ENV_VAR, zone));

        let inner = Arc::new(
            MetastoreClient::create(
                std::slice::from_ref(&label),
                seeds,
                Arc::new(ClusterMetaHostnameResolver::new(zone)),
                creds.as_ref().map(|c| c.client.clone()),
            )
            .await?,
        );
        let db = Arc::new(ProtobufDB::new(inner.clone()));
        Ok(Self {
            zone: zone.to_string(),
            inner,
            db,
            creds,
        })
    }

    pub(crate) fn creds(&self) -> Option<crypto::tls::Credentials> {
        self.creds.clone()
    }

    pub async fn create_from_environment() -> Result<Arc<Self>> {
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

        // TODO: Allow having an insecure cluster?
        // TODO: Add this credentials loader to the resource group.
        let creds = get_cluster_credentials().await?;

        Ok(Arc::new(
            Self::create(
                &zone,
                &seeds,
                Some(crypto::tls::Credentials {
                    server: creds.server_options(),
                    client: creds.client_options(),
                }),
            )
            .await?,
        ))
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

    pub async fn seeds(&self) -> Result<String> {
        let seeds = self.inner().seeds().await;
        Ok(seeds.join(","))
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
