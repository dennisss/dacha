use std::sync::Arc;
use std::time::Duration;

use common::errors::*;
use common::args::list::CommaSeparated;
use cluster_client::ClusterMetaClient;
use file::{LocalPathBuf, LocalPath};
use db_table::db::ProtobufDBTransaction;
use db_table::query;
use cluster_client::acl::principal::Principal;
use cluster_client::service::address::ServiceName;
use container_proto::cluster::*;
use cluster_client::meta::{GroupMembershipTable, CertificateMetadataTable};
use cluster_client::service::create_rpc_channel;
use file::Stdin;
use common::io::Readable;
use cluster_client::env::CREDENTIALS_DIR_ENV_VAR;
use cluster_client::meta::constants::META_STORE_SEEDS_ENV_VAR;
use cluster_client::credentials::LOCALHOST_CERT_DURATION;
use crypto::tls::FileCredentialsManager;
use crypto::x509::{CertificateRegistry, Certificate, CertificateRequestBuilder, PrivateKey, PrivateKeyType, CertificateBuilder};

use crate::create_user_command::read_stdin_password;
use crate::nss::{install_nss_certificates, check_have_nss_utils};
use crate::bridge::setup_bridge;
use crate::chrome_policy::setup_chrome_cert_policy;

#[derive(Args)]
pub struct LoginCommand {
    user_name: String,

    /// Regenerate certificates even if they aren't near expiring.
    #[arg(default = false)]
    force_refresh: bool,
}

pub async fn run_login(cmd: LoginCommand) -> Result<()> {
    check_have_nss_utils().await?;

    // TODO: Need to change to be an anonymous client.
    let meta_client = ClusterMetaClient::create_from_environment().await?;
    let pass = read_stdin_password(false).await?;

    login_impl(meta_client, &cmd.user_name, &pass).await?;

    Ok(())
}

// Assumption is that 'meta_client' has enough permissions to perform the login, dns,
// and certificate lookup operations.
//
// TODO: Ensure that we are always using a 'unauthneticated' meta_client
pub(crate) async fn login_impl(
    meta_client: Arc<ClusterMetaClient>,
    user_name: &str,
    user_password: &str
) -> Result<()> {
    let request_context = rpc::ClientRequestContext::default();

    let ca_channel = create_rpc_channel(
        "cert-authority.system.job.local.cluster.internal",
        meta_client.clone()
    ).await?;

    let auth_service = UserAuthenticationStub::new(ca_channel);


    let name = ServiceName::for_user(meta_client.zone(), user_name)?;

    // TODO: Implement a minimum duration remaining to refresh the certificates..
    /*
    let now = SystemTime::now();
    let not_after = SystemTime::from(certs[0].validity().not_after);
    let time_remaining = not_after.duration_since(now).unwrap_or(Duration::ZERO);
    */


    let private_key =
        Arc::new(PrivateKey::generate(PrivateKeyType::ECDSA_SECP256R1).await?);

    let csr = {
        let mut csr = CertificateRequestBuilder::default();
        let cname = name.to_string();
        csr.set_common_name(&cname);
        csr.build(&private_key).await?
    };
    
    let user_certs = {
        let mut req = LoginRequest::default();
        req.set_user_name(user_name);
        req.set_user_password(user_password);
        req.set_csr(csr.to_der());

        let res = auth_service.Login(&request_context, &req).await.result?;

        let mut certs = vec![];
        for cert_data in res.certificate() {
            certs.push(Arc::new(Certificate::read(cert_data[..].into())?));
        }

        certs
    };

    // Initialize a new registry from the metastore (may include new certificates added since
    // last time we logged in).
    // TODO: De-deduplicate this logic.
    // TODO: Needs to happen after login since we may not have credentials yet.
    let mut registry = cluster_client::credentials::read_latest_certificate_registry(&meta_client).await?;

    let (local_cert, local_key) = create_localhost_identity().await?;
    registry.append(&[local_cert.clone()], true)?;

    let home = std::env::var("HOME")?;

    let credentials_dir = {
        LocalPath::new(&home).join(".dacha/zone").join(meta_client.zone()).join("credentials")
    };
    println!("User Credentials Dir: {}", credentials_dir.as_str());
    file::create_dir_all(&credentials_dir).await?;
    let mut credentials_manager = FileCredentialsManager::create(&credentials_dir).await?;

    // Write all the credentials to disk.
    credentials_manager.write_registry(Arc::new(registry)).await?;
    credentials_manager.write_certificates(&user_certs, private_key.clone()).await?;
    credentials_manager.write_certificates_with_name("localhost", &[local_cert.clone()], local_key.clone()).await?;

    setup_local_zone_files(
        &home, meta_client.zone(), credentials_dir.as_str(), &meta_client.seeds().await?).await?;
    
    install_nss_certificates(&mut credentials_manager).await?;

    credentials_manager.gc().await?;

    setup_bridge(false).await?;

    setup_chrome_cert_policy(&name.to_string()).await?;

    Ok(())
}

async fn create_localhost_identity() -> Result<(Arc<Certificate>, Arc<PrivateKey>)> {
    let private_key = Arc::new(PrivateKey::generate(PrivateKeyType::ECDSA_SECP256R1).await?);
    
    let csr = {
        let mut csr = CertificateRequestBuilder::default();
        csr.set_common_name("localhost")?;
        csr.set_subject_alt_names(&["localhost"])?;
        csr.build(&private_key).await?
    };

    let cert_data = CertificateBuilder::new(
        csr,
        LOCALHOST_CERT_DURATION,
        crypto::x509::SubjectValue::CopyCSR,
    )?
    .create_ca()
    .set_subject_alt_names(crypto::x509::SubjectAltNameValue::CopyCSR)
    .build(None, &private_key)
    .await?;

    let cert = Arc::new(Certificate::read(cert_data.into())?);

    Ok((cert, private_key))
}

pub(super) async fn setup_local_zone_files(home: &str, zone: &str, credentials_dir: &str, meta_seeds: &str) -> Result<()> {
    {
        let mut env = UserEnvProto::default();

        {
            let var = env.new_vars();
            var.set_key(META_STORE_SEEDS_ENV_VAR);
            var.set_value(meta_seeds);
        }

        {
            let var = env.new_vars();
            var.set_key(CREDENTIALS_DIR_ENV_VAR);
            var.set_value(credentials_dir);
        }

        let env_str = protobuf::text::serialize_text_proto(&env);

        file::write(LocalPath::new(&home).join(".dacha/zone").join(zone).join("env"), env_str).await?;
    }

    file::write(LocalPath::new(&home).join(".dacha/default_zone"), zone.to_string()).await?;

    Ok(())
}

