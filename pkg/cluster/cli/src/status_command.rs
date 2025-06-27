use std::time::{SystemTime, Duration};

use common::errors::*;
use cluster_client::ClusterMetaClient;
use crypto::tls::CertificateRegistrySource;
use terminal::TerminalTableBuilder;
use base_units::format_duration_secs;

#[derive(Args)]
pub struct StatusCommand {}

pub async fn run_status(cmd: StatusCommand) -> Result<()> {
    let now = SystemTime::now();
    
    let meta_client = ClusterMetaClient::create_from_environment().await?;
    println!("Zone: {}", meta_client.zone());
    println!("");

    let creds = meta_client.creds().ok_or_else(|| err_msg("No credentials being used for meta client"))?;
    let client_opts = creds.client.get();

    println!("# Identity");
    if client_opts.certificate_auth.identities.is_empty() {
        println!("<Unauthenticated>");
    } else {
        let cert = &client_opts.certificate_auth.identities[0].certificates[0];
        let name = cert.subject().common_name()?.ok_or_else(|| err_msg("No common name in identity"))?;

        let not_after = SystemTime::from(cert.validity().not_after);
        let time_remaining = not_after.duration_since(now).unwrap_or(Duration::ZERO);

        println!("Name:    {}", name);
        println!("Expires: {}", format_duration_secs(time_remaining));
    }
    println!("");


    let registry = {
        match &client_opts.certificate_request.root_certificate_registry {
            CertificateRegistrySource::Custom(v) => v.clone(),
            // We always materialize to ::Custom during the loading process so we should never see
            // this.
            CertificateRegistrySource::PublicRoots => panic!(),
        }
    };

    println!("# Root Certificate Registry");

    let mut table = TerminalTableBuilder::new();

    table.row().col("COMMON NAME").col("EXPIRES IN");
    for cert in registry.certificates() {
        let name = cert.subject().common_name()?.ok_or_else(|| err_msg("No common name in registry certificate"))?;

        let not_after = SystemTime::from(cert.validity().not_after);
        let time_remaining = not_after.duration_since(now).unwrap_or(Duration::ZERO);

        table.row().col(name).col(format_duration_secs(time_remaining));
    }

    table.print();
    println!("");

    // TODO: Check if we have the root key locally available.

    Ok(())
}
