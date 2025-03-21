use std::sync::Arc;

use base_error::*;
use cluster_client::credentials::{cert_duration_for_entity, get_server_peer_identity};
use cluster_client::meta::{PrivateKeyMetadataTable, WorkerMetadataTable};
use cluster_client::{
    meta::{client::ClusterMetaClient, CertificateMetadataTable},
    service::address::{ServiceEntity, ServiceName},
};
use common::bytes::Bytes;
use container_proto::cluster::*;
use crypto::tls::FileCredentialsManager;
use crypto::x509::Certificate;
use db_table::{query, query_one};
use file::LocalPath;

use crate::sign_leaf_certificate;
use crate::utils::insert_certificate_into_registry;

// TODO: Need a background thread to re-new the certificate with a new CSR when
// it becomes stale.

pub struct CertificateAuthorityImpl {
    client: Arc<ClusterMetaClient>,
    creds: Credentials,
}

struct Credentials {
    certificate: Arc<crypto::x509::Certificate>,
    private_key: Arc<crypto::x509::PrivateKey>,
}

impl CertificateAuthorityImpl {
    pub async fn create(client: Arc<ClusterMetaClient>) -> Result<Self> {
        let creds = Self::load_credentials(&client).await?;
        Ok(Self { client, creds })
    }

    // TODO: Eventually need to get a master lock and have a worker refresh root CAs
    // and clean up expired certificates from the DB.

    async fn load_credentials(meta_client: &ClusterMetaClient) -> Result<Credentials> {
        let certs = query!(meta_client.db(), CertificateMetadataTable, "root = TRUE");

        // TODO: Support picking whichever one is unexpired and has existed for
        // sufficiently long.
        if certs.len() != 1 {
            return Err(err_msg("Only 1 root certificate supported"));
        }

        let key = query_one!(
            meta_client.db(),
            PrivateKeyMetadataTable,
            "id = ?",
            certs[0].key_id()
        )
        .ok_or_else(|| err_msg("Can't find the certificate id"))?;

        let certificate = Arc::new(crypto::x509::Certificate::read(certs[0].data().into())?);
        let private_key = Arc::new(crypto::x509::PrivateKey::from_der(key.data().into())?);

        Ok(Credentials {
            certificate,
            private_key,
        })
    }

    async fn sign_certificate_impl(
        &self,
        request: &SignCertificateRequest,
        context: &rpc::ServerRequestContext,
    ) -> Result<SignCertificateResponse> {
        let client_name = get_server_peer_identity(context)?;

        if client_name.zone() != self.client.zone() {
            return Err(rpc::Status::failed_precondition(
                "Not allowed to sign certificates from another zone",
            )
            .into());
        }

        // Currently only nodes are able to request certificates.
        let node_id = match client_name.entity() {
            ServiceEntity::Node { id } => *id,
            _ => {
                return Err(rpc::Status::failed_precondition(
                    "Client not allowed to sign this certificate (not a node",
                )
                .into());
            }
        };

        let csr = crypto::x509::CertificateRequest::from_der(request.csr().into())
            .map_err(|_| rpc::Status::invalid_argument("Failed to parse CSR DER"))?;

        if !csr.verify_signature()? {
            return Err(rpc::Status::invalid_argument("Invalid CSR signature").into());
        }

        let cert_common_name = csr
            .subject_as_common_name()
            .map_err(|_| rpc::Status::invalid_argument("Unsupported subject format"))?
            .ok_or_else(|| {
                rpc::Status::invalid_argument(
                    "CSR subject must only consist of a single common name",
                )
            })?;

        let cert_name = ServiceName::parse(&cert_common_name).map_err(|_| {
            rpc::Status::invalid_argument(format!("Unknown entity name: {}", cert_common_name))
        })?;

        if cert_name.zone() != self.client.zone() {
            return Err(rpc::Status::invalid_argument(
                "Not going to sign certificates for remote zones",
            )
            .into());
        }

        // Validating that the node is allowed to get a certificate for the requested
        // name.
        match cert_name.entity() {
            ServiceEntity::Node { id } => {
                if *id != node_id {
                    return Err(rpc::Status::invalid_argument(
                        "Not allowed to request another node's certificate",
                    )
                    .into());
                }
            }
            ServiceEntity::Job { job_name } => {
                return Err(rpc::Status::invalid_argument(
                    "Not allowed to request a job level certificate",
                )
                .into());
            }
            ServiceEntity::Worker {
                job_name,
                worker_id,
            } => {
                let worker_name = format!("{}.{}", job_name, worker_id);
                let worker_meta = query_one!(
                    self.client.db(),
                    WorkerMetadataTable,
                    "spec.name = ?",
                    worker_name
                )
                .ok_or_else(|| rpc::Status::invalid_argument("No such worker"))?;

                if worker_meta.assigned_node() != node_id {
                    return Err(rpc::Status::invalid_argument(
                        "Worker not assigned to the requesting node.",
                    )
                    .into());
                }
            }
            ServiceEntity::Root => {
                return Err(rpc::Status::invalid_argument(
                    "Not allowed to request a root certificate",
                )
                .into());
            }
        }

        // TODO: Rate limit cerficiate creation.

        let cert = sign_leaf_certificate(
            &cert_name,
            csr,
            &self.creds.certificate,
            &self.creds.private_key,
        )
        .await?;

        insert_certificate_into_registry(self.client.db().as_ref(), &cert, node_id).await?;

        let mut res = SignCertificateResponse::default();
        res.add_certificate(cert.to_der().into());
        // NOTE: We assume that the CA is using a root certificate and doesn't need to
        // be included in the intermediate cert chain.

        Ok(res)
    }
}

#[async_trait]
impl CertificateAuthorityService for CertificateAuthorityImpl {
    async fn SignCertificate(
        &self,
        request: rpc::ServerRequest<SignCertificateRequest>,
        response: &mut rpc::ServerResponse<SignCertificateResponse>,
    ) -> Result<()> {
        response.value = self
            .sign_certificate_impl(&request.value, &request.context)
            .await?;
        Ok(())
    }
}
