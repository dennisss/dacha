use std::sync::Arc;

use common::errors::*;
use db_table::query;

use crypto::x509::{CertificateRegistry, Certificate};

use crate::meta::CertificateMetadataTable;
use crate::ClusterMetaClient;


pub async fn read_latest_certificate_registry(meta_client: &ClusterMetaClient) -> Result<CertificateRegistry> {
    let certs = query!(meta_client.db(), CertificateMetadataTable, "root = true");
    if certs.len() == 0 {
        return Err(err_msg("Unable to find any root certificates"));
    }

    let mut new_registry = CertificateRegistry::new();
    for cert in certs {
        let c = Certificate::read(cert.data().into())?;
        new_registry.append(&[Arc::new(c)], true)?;
    }

    Ok(new_registry)
}