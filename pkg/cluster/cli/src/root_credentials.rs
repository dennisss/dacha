use std::sync::Arc;

use common::errors::*;
use file::{LocalPathBuf, LocalPath};
use crypto::tls::{Credentials, FileCredentialsManager};
use cluster_ca::create_root_credentials;

// TODO: Replace with the file credentials manager
pub struct RootCredentials {
    pub private_key: Arc<crypto::x509::PrivateKey>,
    pub certificate: Arc<crypto::x509::Certificate>,
    pub registry: Arc<crypto::x509::CertificateRegistry>,
    pub tls: crypto::tls::Credentials,
}

pub async fn load_or_create_root_credentials(
    dir: &LocalPath,
    zone: &str,
    bootstrap: bool,
) -> Result<RootCredentials> {
    // TODO: Need to dedup this with the file credentials loader in the crypto
    // package

    file::create_dir_all(dir).await?;

    let mut manager = FileCredentialsManager::create(dir).await?;

    if manager.certificates().is_none() {
        if !bootstrap {
            return Err(err_msg(
                "Will only create a new root key/certificate if --bootstrap=true",
            ));
        }

        let (cert, key) = create_root_credentials(zone).await?;

        let registry = {
            let mut registry = crypto::x509::CertificateRegistry::new();
            registry.append(&[cert.clone()], true)?;
            Arc::new(registry)
        };

        manager.write_registry(registry.clone()).await?;
        manager.write_certificates(&[cert], key.clone()).await?;
    }

    let (certs, pkey) = manager.certificates_with_private_key().unwrap();

    if certs.len() != 1 {
        return Err(err_msg(
            "Expected exactly one root certificate for a single zone",
        ));
    }

    Ok(RootCredentials {
        private_key: pkey.clone(),
        certificate: certs[0].clone(),
        registry: manager.registry().unwrap(),
        tls: Credentials {
            client: manager.client_options().unwrap(),
            server: manager.server_options().unwrap(),
        },
    })
}


pub fn get_root_credentials_dir(zone: &str, user_arg: &Option<LocalPathBuf>) -> Result<LocalPathBuf> {
    let path = {
        if let Some(path) = user_arg.clone() {
            path.clone()
        } else {
            let home = std::env::var("HOME")?;
            LocalPath::new(&home).join(".dacha/zone").join(zone).join("root")
        }
    };
    
    println!("Root Credentials Dir: {}", path.as_str());
    Ok(path)
}