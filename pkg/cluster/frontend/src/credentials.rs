use std::sync::Arc;
use std::time::Duration;

use base_error::*;
use db_table::db::{ProtobufDBTransaction, ProtobufDB};
use db_table::query_one;
use cluster_client::meta::ObjectMetadataTable;
use cluster_client::ClusterMetaClient;
use cluster_proto::cluster::ObjectMetadata;
use executor_multitask::{TaskResource, impl_resource_passthrough};
use crypto::tls::{ServerOptionsContainer, ServerOptions, CertificateIdentity};
use crypto::x509::{PrivateKey, Certificate};


/// Loads public TLS credentials from the metastore.
/// (this is done periodically in the background).
pub struct ObjectCredentialsLoader {
    task: TaskResource,
    shared: Arc<Shared>,
}

impl_resource_passthrough!(ObjectCredentialsLoader, task);

struct Shared {
    meta_client: Arc<ClusterMetaClient>,
    object_prefix: String,
    server_options: ServerOptionsContainer,
}

impl ObjectCredentialsLoader {

    /// Creates the loader with loading the initial value.
    pub async fn create(meta_client: Arc<ClusterMetaClient>, object_prefix: &str) -> Result<Self> {

        let server_options = Self::load_once(&meta_client.db(), object_prefix).await?;

        let shared = Arc::new(Shared {
            meta_client,
            object_prefix: object_prefix.to_string(),
            server_options: server_options.into()
        });

        let task = TaskResource::spawn_interruptable(
            "ObjectCredentialsLoader",
            Self::run(shared.clone()),
        );

        Ok(Self {
            task,
            shared,
        })
    }

    pub fn server_options(&self) -> ServerOptionsContainer {
        self.shared.server_options.clone()
    }

    async fn run(shared: Arc<Shared>) -> Result<()> {
        loop {
            // TODO: Report failures in the resources.
            if let Err(e) = Self::run_impl(&shared).await {
                eprintln!("ObjectCredentialsLoader failure: {}", e);
            }

            executor::sleep(Duration::from_secs(5*60)).await?;
        }

        Ok(())
    }

    async fn run_impl(shared: &Shared) -> Result<()> {
        let options = Self::load_once(&shared.meta_client.db(), &shared.object_prefix).await?;
        shared.server_options.set(options.into());
        Ok(())
    }

    async fn load_once(db: &ProtobufDB, object_prefix: &str) -> Result<ServerOptions> {
        let (certs, private_key) = {
            let cert_object = format!("{}/certificate.pem", object_prefix);
            let key_object = format!("{}/private_key.pem", object_prefix);

            let txn = db.new_transaction().await?;
            let cert_data = Self::get_object(&txn, &cert_object).await?
                .ok_or_else(|| format_err!("Missing certificate object: {}", cert_object))?;
            let key_data = Self::get_object(&txn, &key_object).await?
                .ok_or_else(|| format_err!("Missing key object: {}", key_object))?;

            let certs = Certificate::from_pem(cert_data.into())?;
            let private_key = PrivateKey::from_pem(key_data.into())?;

            (certs, private_key)
        };

        // TODO: Block writing to swap.
        let mut server_options =
            ServerOptions::recommended_with_identities(vec![
                CertificateIdentity {
                    certificates: certs,
                    private_key: Arc::new(private_key)
                }

            ]);

        Ok(server_options)
    }

    // TODO: Dedup this.
    async fn get_object(txn: &ProtobufDBTransaction<'_>, name: &str) -> Result<Option<Vec<u8>>> {
        let obj = query_one!(txn, ObjectMetadataTable, "name = ?", name);
        match obj {
            Some(v) => Ok(Some(v.data().into())),
            None => Ok(None)
        }
    }
}
