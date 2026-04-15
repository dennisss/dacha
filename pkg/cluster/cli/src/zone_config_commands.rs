use std::sync::Arc;

use common::errors::*;
use file::{LocalPathBuf, LocalPath};
use cluster_proto::cluster::{ZoneConfigBackup, ZoneConfigBackupFile};
use crypto::tls::CertificateRegistrySource;
use crypto::tls::FileCredentialsManager;
use cluster_client::ClusterMetaClient;
use crypto::x509::{Certificate, CertificateRegistry, PrivateKey};
use protobuf::Message;

use crate::login_command::setup_local_zone_files;
use crate::root_credentials::get_root_credentials_dir;


#[derive(Args)]
pub struct SaveZoneConfigCommand {
    #[arg(positional)]
    path: LocalPathBuf,

    #[arg(default = false)]
    include_root_creds: bool,

    /// If true, make no attempts to retrieve new data (e.g. seed ips or
    /// certificate registry data) from servers in the cluster (in
    /// other words, don't contact any network devices when running this command).
    ///
    /// TODO: Implement me.
    #[arg(default = false)]
    offline: bool
}

#[derive(Args)]
pub struct LoadZoneConfigCommand {
    #[arg(positional)]
    path: LocalPathBuf,
}

#[derive(Args)]
pub struct SetDefaultZoneCommand {
    zone: String
}

pub async fn run_save_zone_config(cmd: SaveZoneConfigCommand) -> Result<()> {
    let meta_client = ClusterMetaClient::create_from_environment().await?;

    println!("Saving zone '{}'", meta_client.zone());

    // let registry = cluster_client::credentials::read_latest_certificate_registry()

    // TODO: Instead read the latest one from the metastore.
    let registry = {
        let creds = meta_client.creds().ok_or_else(|| err_msg("No credentials being used for meta client"))?;
        let client_opts = creds.client.get();

        match &client_opts.certificate_request.root_certificate_registry {
            CertificateRegistrySource::Custom(v) => v.clone(),
            // We always materialize to ::Custom during the loading process so we should never see
            // this.
            CertificateRegistrySource::PublicRoots => panic!(),
        }
    };

    let mut out = ZoneConfigBackup::default();
    out.set_zone_name(meta_client.zone());

    // NOTE: Should be done after any metastore operations to ensure that we have the latest seeds.
    out.set_meta_seeds(meta_client.seeds().await?);

    for cert in registry.certificates() {
        let name = cert.subject().common_name()?.ok_or_else(|| err_msg("No common name in registry certificate"))?;

        // Skip things like 'localhost' entries.
        if !name.ends_with(".cluster.internal") {
            continue;
        }

        out.add_tls_registry(cert.to_der().into());
    }

    if out.tls_registry().is_empty() {
        return Err(err_msg("Empty TLS registry saved."));
    }

    // TODO: May also need to retrieve latest root credentials from the DB.
    // TODO: Eventually deprecate savings these and instead always pull
    // latest credentials from the metastore when this is needed.
    if cmd.include_root_creds {
        println!("Including root identity secrets...");

        let root_creds_dir = get_root_credentials_dir(&meta_client.zone(), &None)?;
        println!("Root Credentials Dir: {}", root_creds_dir.as_str());

        if !file::exists(&root_creds_dir).await? {
            return Err(format_err!("Root credential directory doesn't exist"));
        }

        let root_creds = FileCredentialsManager::create(&root_creds_dir).await?;

        let (certs, pkey) = root_creds.certificates_with_private_key()
            .ok_or_else(|| err_msg("No root credentials found locally"))?;

        out.root_credentials_mut().set_private_key(pkey.to_der());
        for cert in certs {
            out.root_credentials_mut().add_certificates(cert.to_der().into());
        }
    }

    let mut f = ZoneConfigBackupFile::default();
    f.data_mut().pack_from(&out)?;
    file::write(cmd.path, f.serialize()?).await?;

    Ok(())
}

pub async fn run_load_zone_config(cmd: LoadZoneConfigCommand) -> Result<()> {
    let mut f = ZoneConfigBackupFile::default();
    f.parse_merge(&file::read(cmd.path).await?[..])?;

    let data = f.data().unpack::<ZoneConfigBackup>()?
        .ok_or_else(|| err_msg("Unable to unpack zone backup data"))?;

    println!("Restoring zone '{}'", data.zone_name());
    if !cluster_client::service::zone::is_valid_zone(data.zone_name()) {
        return Err(err_msg("Invalid zone name"));
    }

    if data.tls_registry().is_empty() {
        return Err(err_msg("Empty certificate registry"));
    }

    let registry = {
        let mut registry = CertificateRegistry::new();
        for cert in data.tls_registry() {
            let cert = Arc::new(Certificate::read(cert.as_ref().into())?);
            registry.append(&[cert], true)?;
        }

        Arc::new(registry)
    };

    let home = std::env::var("HOME")?;

    let credentials_dir = {
        LocalPath::new(&home).join(".dacha/zone").join(data.zone_name()).join("credentials")
    };
    println!("User Credentials Dir: {}", credentials_dir.as_str());
    file::create_dir_all(&credentials_dir).await?;

    {
        let mut credentials_manager = FileCredentialsManager::create(&credentials_dir).await?;
        credentials_manager.write_registry(registry.clone()).await?;
    }

    if data.has_root_credentials() {
        let root_creds_dir = get_root_credentials_dir(data.zone_name(), &None)?;
        println!("Root Credentials Dir: {}", root_creds_dir.as_str());

        file::create_dir_all(&root_creds_dir).await?;

        let mut manager = FileCredentialsManager::create(&root_creds_dir).await?;
        manager.write_registry(registry.clone()).await?;

        let mut certs = vec![];
        for cert in data.root_credentials().certificates() {
            let cert = Arc::new(Certificate::read(cert.as_ref().into())?);
            certs.push(cert);
        }

        let key = Arc::new(PrivateKey::from_der(data.root_credentials().private_key().as_ref().into())?);

        manager.write_certificates(&certs, key).await?;
    }

    setup_local_zone_files(
        &home, data.zone_name(), credentials_dir.as_str(), data.meta_seeds()).await?;

    // TODO: Warn if already setup and if we have an identity.

    Ok(())
}

pub async fn run_set_default_zone(cmd: SetDefaultZoneCommand) -> Result<()> {
    // TODO: This basically needs to do the same stuff as the login command without logging in.
    // It's trigger if we move to a zone without credentials as we may want to turn off the bridge.

    todo!()
}

