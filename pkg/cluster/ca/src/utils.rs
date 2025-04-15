use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use base_error::*;
use cluster_client::credentials::cert_duration_for_entity;
use cluster_client::{
    meta::CertificateMetadataTable,
    service::address::{ServiceEntity, ServiceName},
    CertificateMetadata,
};
use common::bytes::Bytes;
use common::chrono::{DateTime, Utc};
use crypto::x509::{Certificate, CertificateRequest, PrivateKey};
use db_table::db::ProtobufDB;

pub async fn create_root_credentials(zone: &str) -> Result<(Arc<Certificate>, Arc<PrivateKey>)> {
    let key = Arc::new(
        crypto::x509::PrivateKey::generate(crypto::x509::PrivateKeyType::ECDSA_SECP256R1).await?,
    );

    let name = ServiceName::for_root(zone)?;

    let mut csr = crypto::x509::CertificateRequestBuilder::default();
    csr.set_common_name(&name.to_string())?;

    csr.set_subject_alt_names(&[
        format!("root.{}.cluster.internal", zone),
        // Mainly for bootstrapping to allow running the first node.
        // TODO: Get rid of this.
        format!("meta.system.job.{}.cluster.internal", zone),
    ]);

    let csr = csr.build(&key).await?;

    let duration = cert_duration_for_entity(&name.entity())
        .ok_or_else(|| err_msg("Don't know how to sign this"))?;

    let cert_data =
        crypto::x509::CertificateBuilder::new(csr, duration, crypto::x509::SubjectValue::CopyCSR)?
            .set_subject_alt_names(crypto::x509::SubjectAltNameValue::CopyCSR)
            .create_ca()
            .set_permitted_subtrees(&[format!("{}.cluster.internal", zone)])
            .build(None, &key)
            .await?;

    let cert = Arc::new(crypto::x509::Certificate::read(cert_data.into())?);

    Ok((cert, key))
}

/// Assuming that the given csr is valid, creates a certificate for the given
/// name using the CSR's public key.
///
/// NOTE: You normally must call insert_certificate_into_registry after running
/// this.
pub async fn sign_leaf_certificate(
    name: &ServiceName,
    csr: CertificateRequest,
    ca_certificate: &Certificate,
    ca_private_key: &PrivateKey,
) -> Result<Arc<Certificate>> {
    let subject_name = name.to_string();
    let mut subject_alt_names = vec![subject_name.clone()];

    match name.entity() {
        ServiceEntity::Node { id } => {}
        ServiceEntity::Worker {
            job_name,
            worker_id,
        } => {
            subject_alt_names.push(ServiceName::for_job(name.zone(), &job_name)?.to_string());
        }
        ServiceEntity::User { .. } => {}
        ServiceEntity::Job { .. } | ServiceEntity::Root => panic!(),
    }

    let duration = cert_duration_for_entity(&name.entity())
        .ok_or_else(|| err_msg("Don't know how to sign this"))?;

    let cert_raw = Bytes::from(
        crypto::x509::CertificateBuilder::new(
            csr,
            duration,
            crypto::x509::SubjectValue::CommonName(subject_name),
        )?
        .set_subject_alt_names(crypto::x509::SubjectAltNameValue::DNSNames(
            subject_alt_names,
        ))
        .build(Some(ca_certificate), ca_private_key)
        .await?,
    );

    let cert = Certificate::read(cert_raw.clone().into())?;

    Ok(Arc::new(cert))
}

pub async fn insert_certificate_into_registry(
    db: &ProtobufDB,
    cert: &Certificate,
    assigned_node: u64,
) -> Result<()> {
    let mut entry = CertificateMetadata::default();
    entry.set_serial_number(cert.serial_number().to_be_bytes());
    entry.set_data(cert.to_der());
    entry.set_assigned_node(assigned_node);
    entry.set_creation_time(to_micros(cert.validity().not_before));
    entry.set_expiration_time(to_micros(cert.validity().not_after));

    entry.set_reversed_common_name({
        let cname = cert
            .subject()
            .common_name()?
            .ok_or_else(|| err_msg("Cert missing common name"))?;

        let mut parts = cname.split('.').collect::<Vec<_>>();
        parts.reverse();

        parts.join(".")
    });

    entry.set_key_id(cert.subject_key_id());

    entry.set_root(cert.self_signed());

    db.insert::<CertificateMetadataTable>(&entry).await?;

    Ok(())
}

fn to_micros(t: DateTime<Utc>) -> u64 {
    let t = SystemTime::from(t);
    t.duration_since(UNIX_EPOCH).unwrap().as_micros() as u64
}
