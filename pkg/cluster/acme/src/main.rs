#[macro_use]
extern crate macros;

use std::sync::Arc;
use std::time::Duration;
use std::time::SystemTime;

use base_error::*;
use base_units::format_duration_secs;
use common::bytes::Bytes;
use executor_multitask::RootResource;
use cluster_client::ClusterMetaClient;
use cluster_client::meta::ObjectMetadataTable;
use cluster_client::ClusterServer;
use cluster_proto::cluster::ObjectMetadata;
use rpc_util::NamedPortArg;
use db_table::query_one;
use db_table::db::ProtobufDBTransaction;
use google_auth::GoogleServiceAccount;


/// Maximum time between certificate refresh attempts.
const MIN_POLL_INTERVAL: Duration = Duration::from_secs(1 * 60 * 60);

/// Minimum time between certificate refresh attempts.
const MAX_POLL_INTERVAL: Duration = Duration::from_secs(30);

#[derive(Args)]
struct Args {
    port: NamedPortArg,
    refresher: RefresherArgs
}

#[derive(Args)]
struct RefresherArgs {
    acme_server: ACMEServer,

    /// DNS name for which to request certificates
    /// e.g. 'google.com'
    dns_name: String,

    /// Name of the metastore object containing a GCP service account to
    /// use for DNS authentication.  
    google_service_account_object: String,

    /// Object prefix for storing data in the metastore.
    ///
    /// - '{prefix}/account_key.pem' is the ACME account private key in PEM format.
    /// - '{prefix}/out/certificate.pem' is the latest generated certificate chain in PEM format 
    /// - '{prefix}/out/private_key.pem' is the latest generated private key in PEM format for the above certificate.
    ///
    /// TODO: Need to acquire a lock to ensure that no other workers are simultaneously obtaining new credentials.
    output_object_prefix: String,

    /// Refresh the certificate when it has less than this much time left before expiring.
    min_remaining_duration: Duration,
}

#[derive(Args)]
pub enum ACMEServer {
    #[arg(name = "letsencrypt-prod")]
    LetsEncryptProd,

    #[arg(name = "letsencrypt-staging")]
    LetsEncryptStaging,
}


const SERVICE_ACL_PROTO: &'static str = r#"
    rules: []
"#;

async fn get_object(txn: &ProtobufDBTransaction<'_>, name: &str) -> Result<Option<Vec<u8>>> {
    let obj = query_one!(txn, ObjectMetadataTable, "name = ?", name);
    match obj {
        Some(v) => Ok(Some(v.data().into())),
        None => Ok(None)
    }
}

async fn set_object(txn: &mut ProtobufDBTransaction<'_>, name: &str, data: &[u8]) -> Result<()> {
    let mut obj = ObjectMetadata::default();
    obj.set_name(name);
    obj.set_data(data);
    txn.put::<ObjectMetadataTable>(&obj).await
}

/// If the certificate is still valid, then returns how long we should wait before re-checking.
async fn check_existing_certificate(txn: &ProtobufDBTransaction<'_>, args: &RefresherArgs) -> Result<Option<Duration>> {
    // TODO: Dedup this.
    let cert_obj = format!("{}/out/certificate.pem", args.output_object_prefix);

    let data = match get_object(&txn, &cert_obj).await? {
        Some(v) => v,
        None => return Ok(None)
    };

    let certs = crypto::x509::Certificate::from_pem(data.into())?;

    // TODO: Validate that it matches our desired common name / alt name.

    let now = SystemTime::now();
    let not_before = SystemTime::from(certs[0].validity().not_before);
    let not_after = SystemTime::from(certs[0].validity().not_after);
    let time_remaining = not_after.duration_since(now).unwrap_or(Duration::ZERO);

    // If the certificate expires too quickly we may get into an infinite loop of refreshing it.
    let validity_period = not_after.duration_since(not_before).unwrap();
    if validity_period <= args.min_remaining_duration + MIN_POLL_INTERVAL {
        return Err(format_err!("Very short certificate period: {}", format_duration_secs(validity_period)));
    }

    if time_remaining <= args.min_remaining_duration {
        println!("Certificate expiring soon. Will refresh. Remaining time: {}", format_duration_secs(time_remaining));
        return Ok(None);
    }

    println!("Re-using existing certificate. Remaining time: {} > {}",
        format_duration_secs(time_remaining), format_duration_secs(args.min_remaining_duration));

    Ok(Some(time_remaining.max(MAX_POLL_INTERVAL).min(MIN_POLL_INTERVAL)))
}

// NOTE: Retrying on failures will happen with restarting the worker.
async fn run(args: RefresherArgs, client: Arc<ClusterMetaClient>) -> Result<()> {

    let mut txn = client.db().new_transaction().await?;
    let sa_data = String::from_utf8(get_object(&txn, &args.google_service_account_object).await?
        .ok_or_else(|| err_msg("No service account found"))?)?;

    let sa: Arc<GoogleServiceAccount> =
        Arc::new(GoogleServiceAccount::parse_json(&sa_data)?);

    let account_key_obj = format!("{}/account_key.pem", args.output_object_prefix);
    let cert_obj = format!("{}/out/certificate.pem", args.output_object_prefix);
    let private_key_obj = format!("{}/out/private_key.pem", args.output_object_prefix);

    let account_private_key = {
        if let Some(data) = get_object(&txn, &account_key_obj).await? {
            crypto::x509::PrivateKey::from_pem(data.into())?
        } else {
            println!("Creating new account key...");
            let key = crypto::x509::PrivateKey::generate(crypto::x509::PrivateKeyType::ECDSA_SECP256R1)
                .await?;

            set_object(&mut txn, &account_key_obj, key.to_pem().as_bytes()).await?;
            key
        }
    };

    txn.commit().await?;

    let rest_client = Arc::new(google_auth::GoogleRestClient::create(sa.clone())?);
    let dns_client = google_dns::Client::new(sa.project_id(), rest_client)?;

    // TODO: Immedately try querying DNS to verify we have access.

    let mut solvers: Vec<Arc<dyn acme::ACMEChallengeSolver>> = vec![];
    solvers.push(Arc::new(acme::GoogleDNSSolver::new(dns_client)));

    loop {
        let mut txn = client.db().new_transaction().await?;

        if let Some(wait_time) = check_existing_certificate(&txn, &args).await? {
            executor::sleep(wait_time).await?;
            continue;
        }

        println!("Requesting new certificate...");

        // TODO: Store the private key before the rest of the stuff in case we need to retry.
        let csr_private_key = crypto::x509::PrivateKey::generate(
            crypto::x509::PrivateKeyType::ECDSA_SECP256R1).await?;

        let mut csr = crypto::x509::CertificateRequestBuilder::default()
            .set_common_name(&args.dns_name)?
            .set_subject_alt_names(&[args.dns_name.clone(), format!("*.{}", args.dns_name)])?
            .build(&csr_private_key)
            .await?;

        let url = match args.acme_server {
            ACMEServer::LetsEncryptProd => acme::LETSENCRYPT_PROD_DIRECTORY,
            ACMEServer::LetsEncryptStaging => acme::LETSENCRYPT_STAGING_DIRECTORY,
        };

        let client = acme::ACMEClient::create(
            url,
            solvers.clone(),
            &account_private_key,
            acme::ACMEClientOptions::default(),
        )
        .await?;

        let cert_pem: Bytes = client.request_certificate(&csr).await?.into();

        let certs = crypto::x509::Certificate::from_pem(cert_pem.clone())?;

        println!("New certificate generated. Serial number: {}",
            base_radix::hex_encode(&certs[0].serial_number().to_be_bytes()));

        // NOTE: Both the cert and private key are simultaneously updated so no syncronization is required by peers.
        set_object(&mut txn, &cert_obj, &cert_pem).await?;
        set_object(&mut txn, &private_key_obj, csr_private_key.to_pem().as_bytes()).await?;
        txn.commit().await?;

        executor::sleep(MAX_POLL_INTERVAL).await?;
    }


    Ok(())
}

#[executor_main]
async fn main() -> Result<()> {
    let args = common::args::parse_args::<Args>()?;
    
    let service = RootResource::new();

    let client = ClusterMetaClient::create_from_environment().await?;
    service.register_dependency(client.clone()).await;

    let mut acl = cluster_proto::cluster::ServiceACLProto::default();
    protobuf::text::parse_text_proto(SERVICE_ACL_PROTO, &mut acl)?;

    let mut server = ClusterServer::new(args.port.value(), acl, client.clone())?;

    service.register_dependency(server.start()?).await;

    service.spawn_interruptable("Refresher", run(args.refresher, client)).await;

    service.wait().await
}



