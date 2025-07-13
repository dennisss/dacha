use core::ops::{Deref, DerefMut};
use std::sync::Arc;
use std::collections::HashMap;

use common::errors::*;
use container_proto::cluster::{ObjectMetadata, UserEnvProto};
use db_txn_client::TransactionalDBClient;
use db_table::db::ProtobufDB;
use db_table::query_one;
use executor_multitask::{impl_resource_passthrough, ServiceResourceGroup};
use protobuf::{Message, StaticMessage};
use protobuf_builtins::google::protobuf::Any;
use raft_client::proto::RouteLabel;
use crypto::tls::FileCredentialsLoader;
use file::LocalPath;

use crate::env::ZONE_ENV_VAR;
use crate::env::CREDENTIALS_DIR_ENV_VAR;
use crate::meta::ObjectMetadataTable;

use super::constants::META_STORE_SEEDS_ENV_VAR;
use super::hostname::ClusterMetaHostnameResolver;

///
pub struct ClusterMetaClient {
    zone: String,
    inner: Arc<TransactionalDBClient>,
    db: Arc<ProtobufDB>,
    creds: Option<crypto::tls::Credentials>,
    resources: ServiceResourceGroup,
}

impl_resource_passthrough!(ClusterMetaClient, resources);

impl ClusterMetaClient {
    pub async fn create(
        zone: &str,
        seeds: &[String],
        creds: Option<crypto::tls::Credentials>,
        loader: Option<Arc<FileCredentialsLoader>>,
    ) -> Result<Self> {
        let resources = ServiceResourceGroup::new("ClusterMetaClient");

        let mut label = RouteLabel::default();
        label.set_value(format!("{}={}", ZONE_ENV_VAR, zone));

        let inner = Arc::new(
            TransactionalDBClient::create(
                std::slice::from_ref(&label),
                seeds,
                Arc::new(ClusterMetaHostnameResolver::new(zone)),
                creds.as_ref().map(|c| c.client.clone()),
            )
            .await?,
        );
        resources.register_dependency(inner.clone()).await;

        if let Some(loader) = loader {
            resources.register_dependency(loader).await;
        }

        let db = Arc::new(ProtobufDB::new(inner.clone()));
        Ok(Self {
            zone: zone.to_string(),
            inner,
            db,
            creds,
            resources
        })
    }

    pub fn creds(&self) -> Option<crypto::tls::Credentials> {
        self.creds.clone()
    }

    pub async fn create_from_environment() -> Result<Arc<Self>> {
        let zone = Self::zone_from_environment().await?;
        let env = EnvVarsOverlay::create(&zone).await?;

        let mut seeds = env.get(META_STORE_SEEDS_ENV_VAR)
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


        let creds;

        if std::env::var("CLUSTER_SUDO").is_ok() {
            // TODO: Check for "CLUSTER_SUDO=yes"

            let home = std::env::var("HOME")?;
            let dir = LocalPath::new(&home).join(".dacha/zone").join(&zone).join("root");
            // TODO: Check dir exists.

            creds = Arc::new(FileCredentialsLoader::create(&dir).await?);

            // TODO: Check we have a certificate and registry in the 'creds'

        } else {
            // TODO: Allow having an insecure cluster?
            let dir = env.get(CREDENTIALS_DIR_ENV_VAR)?;
            creds = Arc::new(FileCredentialsLoader::create(LocalPath::new(&dir)).await?);
        }


        Ok(Arc::new(
            Self::create(
                &zone,
                &seeds,
                Some(crypto::tls::Credentials {
                    server: creds.server_options(),
                    client: creds.client_options(),
                }),
                Some(creds)
            )
            .await?,
        ))
    }

    async fn zone_from_environment() -> Result<String> {
        if let Ok(v) = std::env::var(ZONE_ENV_VAR) {
            return Ok(v);
        }

        if let Ok(home) = std::env::var("HOME") {
            let default_path = LocalPath::new(&home).join(".dacha/default_zone");
            if file::exists(&default_path).await? {
                return file::read_to_string(&default_path).await;
            }
        }

        if std::env::var("SHELL").is_ok() {
            // TODO: Improve user feedback.
            return Err(err_msg("Not logged in to any zone."));
        }

        Err(format_err!(
            "Expected the {} environment variable to be set",
            ZONE_ENV_VAR,
        ))
    }

    pub fn zone(&self) -> &str {
        &self.zone
    }

    pub fn inner(&self) -> &TransactionalDBClient {
        &self.inner
    }

    // TODO: If the ClusterMetaClient is dropped, then this needs to stop working.
    pub fn db(&self) -> &Arc<ProtobufDB> {
        &self.db
    }

    /// Makes a client instance that shared the same zone level parameters like
    /// certificate registries, but does not have any client/server identities.
    ///
    /// NOTE: This is only meant for temporary usage and doesn't do stuff like
    /// reloading of certificates from disk.
    pub async fn clone_unauthenticated(&self) -> Result<Self> {
        let creds = self.creds.as_ref().map(|creds| {
            let mut server = creds.server.get().as_ref().clone();
            let mut client = creds.client.get().as_ref().clone();

            server.certificate_auth.identities.clear();
            client.certificate_auth.identities.clear();

            crypto::tls::Credentials { server: server.into(), client: client.into() }
        });

        let seeds = self.inner().seeds().await;

        Self::create(
            &self.zone,
            &seeds,
            creds,
            None
        ).await
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

    pub async fn get_object_proto<M: protobuf::Message + Default>(
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

    pub async fn set_object_proto<M: protobuf::Message>(&self, name: &str, value: &M) -> Result<()> {
        let mut obj = ObjectMetadata::default();
        obj.set_name(name);
        obj.value_mut().pack_from(value)?;

        self.db().insert::<ObjectMetadataTable>(&obj).await?;

        Ok(())
    }
}

struct EnvVarsOverlay {
    user_vars: HashMap<String, String>
}

impl EnvVarsOverlay {
    pub async fn create(zone: &str) -> Result<Self> {
        let mut user_vars = HashMap::new();
        if let Ok(home) = std::env::var("HOME") {
            let path = LocalPath::new(&home).join(".dacha/zone").join(zone).join("env");
            if file::exists(&path).await? {
                let data = file::read_to_string(&path).await?;
                let mut proto = UserEnvProto::default();
                protobuf::text::parse_text_proto(&data, &mut proto)?;

                for var in proto.vars() {
                    user_vars.insert(var.key().to_string(), var.value().to_string());
                }
            }
        }

        Ok(Self { user_vars })
    }

    pub fn get(&self, name: &str) -> Result<String> {
        if let Ok(v) = std::env::var(name) {
            return Ok(v);
        }

        if let Some(v) = self.user_vars.get(name) {
            return Ok(v.clone());
        }

        Err(format_err!("Unable to find environment variable: {}", name))
    }

}

