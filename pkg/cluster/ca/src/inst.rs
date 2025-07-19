use std::sync::Arc;
use std::time::Duration;

use base_error::*;
use cluster_client::credentials::{cert_duration_for_entity};
use cluster_client::{ClusterServerConnectionData, ClusterServerRequestData};
use cluster_client::meta::{PrivateKeyMetadataTable, WorkerMetadataTable, UserTable};
use cluster_client::{
    meta::{client::ClusterMetaClient, CertificateMetadataTable},
    service::address::{ServiceEntity, ServiceName},
};
use common::bytes::Bytes;
use container_proto::cluster::*;
use crypto::tls::FileCredentialsManager;
use crypto::x509::Certificate;
use crypto::bcrypt::*;
use db_table::{query, query_one};
use file::LocalPath;
use cluster_client::throttler::HashedTokenBucketThrottler;

use crate::utils::{insert_certificate_into_registry, sign_leaf_certificate};
use crate::user::*;

// TODO: Need a background thread to re-new the certificate with a new CSR when
// it becomes stale.

pub struct CertificateAuthorityImpl {
    client: Arc<ClusterMetaClient>,
    creds: Credentials,
    throttler: HashedTokenBucketThrottler
}

struct Credentials {
    certificate: Arc<crypto::x509::Certificate>,
    certificate_bytes: Bytes,
    private_key: Arc<crypto::x509::PrivateKey>,
}

impl CertificateAuthorityImpl {
    pub async fn create(client: Arc<ClusterMetaClient>) -> Result<Self> {
        let creds = Self::load_credentials(&client).await?;

        // Roughly 1 QPS per client.
        let throttler = HashedTokenBucketThrottler::create(
            128,
            10,
            Duration::from_secs(10)
        ).await;

        Ok(Self { client, creds, throttler })
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

        let certificate_bytes: Bytes = certs[0].data().into();
        let certificate = Arc::new(crypto::x509::Certificate::read(certificate_bytes.clone())?);
        let private_key = Arc::new(crypto::x509::PrivateKey::from_der(key.data().into())?);

        Ok(Credentials {
            certificate,
            certificate_bytes,
            private_key,
        })
    }

    async fn sign_certificate_impl(
        &self,
        request: &SignCertificateRequest,
        context: &rpc::ServerRequestContext,
    ) -> Result<SignCertificateResponse> {
        let conn = ClusterServerConnectionData::from_rpc_context(context)?;
        let client_name = conn.peer.as_ref().ok_or_else(|| err_msg("No client identity"))?;

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

        // NOTE: We intentionally do this after filtering out inappropriate clients.
        if !self.throttler.take(client_name.to_string().as_bytes(), 1) {
            return Err(rpc::Status::resource_exhausted("Exceeded per-client signing rate limit.").into());
        }

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
            ServiceEntity::User { .. } => {
                return Err(rpc::Status::invalid_argument(
                    "Not allowed to request a user certificate",
                )
                .into());
            }
            ServiceEntity::Root => {
                return Err(rpc::Status::invalid_argument(
                    "Not allowed to request a root certificate",
                )
                .into());
            }
        }

        // TODO: Rate limit cerficiate creation (max 5 with the same name).

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

    async fn get_certificate_registry_impl(
        &self,
        request: &GetCertificateRegistryRequest,
        context: &rpc::ServerRequestContext,
    ) -> Result<GetCertificateRegistryResponse> {
        let mut res = GetCertificateRegistryResponse::default();
        res.add_registry(self.creds.certificate_bytes.as_ref().into());
        Ok(res)
    }

    async fn create_user_impl(
        &self,
        request: &CreateUserRequest,
        context: &rpc::ServerRequestContext,
    ) -> Result<CreateUserResponse> {
        // NOTE: Authentication is done via the ServiceACL

        if !self.throttler.take(b"create_user", 1) {
            return Err(rpc::Status::resource_exhausted("Exceeded CreateUser rate limit.").into());
        }

        let mut txn = self.client.db().new_transaction().await?;

        let existing_user = query_one!(txn, UserTable, "name = ?", request.user_name());
        if existing_user.is_some() {
            return Err(rpc::Status::already_exists("User already exists").into());
        }

        let mut user = User::default();
        user.set_name(request.user_name());

        let digest = bcrypt_encode(request.user_password())
            .map_err(|_| rpc::Status::invalid_argument("Invalid password"))?;

        user.set_password_digest(digest);

        txn.put::<UserTable>(&user).await?;

        txn.commit().await?;

        Ok(CreateUserResponse::default())
    }

    async fn login_impl(
        &self,
        request: &LoginRequest,
        context: &rpc::ServerRequestContext,
    ) -> Result<LoginResponse> {
        if !self.throttler.take(request.user_name().as_bytes(), 1) {
            return Err(rpc::Status::resource_exhausted("Exceeded per-client signing rate limit.").into());
        }

        let user = get_user_with_password(
            request.user_name(),
            request.user_password(),
            &self.client.db().new_transaction().await?
        ).await?;

        let csr = crypto::x509::CertificateRequest::from_der(request.csr().into())
            .map_err(|_| rpc::Status::invalid_argument("Failed to parse CSR DER"))?;

        if !csr.verify_signature()? {
            return Err(rpc::Status::invalid_argument("Invalid CSR signature").into());
        }

        let cert_name = ServiceName::for_user(self.client.zone(), user.name())
            .map_err(|_| rpc::Status::invalid_argument("Invalid user name"))?;

        // TODO: Limit max number of unexpired certificates for the user.

        let cert = sign_leaf_certificate(
            &cert_name,
            csr,
            &self.creds.certificate,
            &self.creds.private_key,
        )
        .await?;

        insert_certificate_into_registry(self.client.db().as_ref(), &cert, 0).await?;

        let mut res = LoginResponse::default();
        res.add_certificate(cert.to_der().into());

        Ok(res)
    }

    async fn change_password_impl(
        &self,
        request: &ChangePasswordRequest,
        context: &rpc::ServerRequestContext,
    ) -> Result<ChangePasswordResponse> {
        if !self.throttler.take(request.user_name().as_bytes(), 1) {
            return Err(rpc::Status::resource_exhausted("Exceeded per-client signing rate limit.").into());
        }

        // NOTE: We intentionally are looking at the peer user and not the
        // effective user since we only want this path without 'current password'
        // validations to be useable by users who have a full certificate. 
        let conn = ClusterServerConnectionData::from_rpc_context(context)?;

        // Current password check not required since we verify that the peer directly has a valid certificate.
        change_user_password(
            request,
            false,
            conn.peer.as_ref(),
            &self.client
        ).await?;

        Ok(ChangePasswordResponse::default())
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

    async fn GetCertificateRegistry(
        &self,
        request: rpc::ServerRequest<GetCertificateRegistryRequest>,
        response: &mut rpc::ServerResponse<GetCertificateRegistryResponse>,
    ) -> Result<()> {
        response.value = self
            .get_certificate_registry_impl(&request.value, &request.context)
            .await?;
        Ok(())
    }
}

#[async_trait]
impl UserAuthenticationService for CertificateAuthorityImpl {
    async fn CreateUser(
        &self,
        request: rpc::ServerRequest<CreateUserRequest>,
        response: &mut rpc::ServerResponse<CreateUserResponse>,
    ) -> Result<()> {
        response.value = self
            .create_user_impl(&request.value, &request.context)
            .await?;
        Ok(())
    }

    async fn Login(
        &self,
        request: rpc::ServerRequest<LoginRequest>,
        response: &mut rpc::ServerResponse<LoginResponse>,
    ) -> Result<()> {
        response.value = self
            .login_impl(&request.value, &request.context)
            .await?;
        Ok(())
    }

    async fn ChangePassword(
        &self,
        request: rpc::ServerRequest<ChangePasswordRequest>,
        response: &mut rpc::ServerResponse<ChangePasswordResponse>,
    ) -> Result<()> {
        response.value = self.change_password_impl(&request.value, &request.context).await?;
        Ok(())
    }
}

