#[macro_use]
extern crate macros;
#[macro_use]
extern crate file;

use std::sync::Arc;

use common::errors::*;
use file::LocalPath;
use google_auth::GoogleServiceAccount;

// TODO: Need alerts on when certificates are close to being invalid without
// success.

async fn find_or_create_private_key(path: &LocalPath) -> Result<crypto::x509::PrivateKey> {
    if file::exists(path).await? {
        let data = file::read(path).await?;
        Ok(crypto::x509::PrivateKey::from_pem(data.into())?)
    } else {
        let key = crypto::x509::PrivateKey::generate(crypto::x509::PrivateKeyType::ECDSA_SECP256R1)
            .await?;
        file::write(path, &key.to_pem()).await?;
        Ok(key)
    }
}

#[executor_main]
async fn main() -> Result<()> {
    let data =
        file::read_to_string("/home/dennis/.credentials/dacha-main-748d2acba112.json").await?;

    let sa: Arc<GoogleServiceAccount> =
        Arc::new(google_auth::GoogleServiceAccount::parse_json(&data)?);

    let rest_client = Arc::new(google_auth::GoogleRestClient::create(sa.clone())?);

    let dns_client = google_dns::Client::new(sa.project_id(), rest_client)?;

    let account_private_key = find_or_create_private_key(&project_path!("acme_test.key")).await?;

    let csr_private_key = find_or_create_private_key(&project_path!("acme_csr.key")).await?;

    let mut csr = crypto::x509::CertificateRequestBuilder::default()
        .set_common_name("dacha.page")?
        .set_subject_alt_names(&["dacha.page"])?
        .build(&csr_private_key)
        .await?;

    let mut solvers: Vec<Arc<dyn acme::ACMEChallengeSolver>> = vec![];

    solvers.push(Arc::new(acme::GoogleDNSSolver::new(dns_client)));

    let client = acme::ACMEClient::create(
        acme::LETSENCRYPT_STAGING_DIRECTORY,
        solvers,
        &account_private_key,
        acme::ACMEClientOptions::default(),
    )
    .await?;

    let cert = client.request_certificate(&csr).await?;

    file::write(project_path!("acme_cert.pem"), cert).await?;

    Ok(())
}
