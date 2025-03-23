use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;
use core::time::Duration;
use std::borrow::ToOwned;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::SystemTime;

use base_error::*;
use executor_multitask::{impl_resource_passthrough, TaskResource};
use file::{LocalFileWatcher, LocalPath, LocalPathBuf};

use crate::tls::options::*;
use crate::tls::options_containers::{ClientOptionsContainer, ServerOptionsContainer};
use crate::x509::{Certificate, CertificateRegistry, PrivateKey};

use super::client;

const CERTIFICATE_FILE: &'static str = "certificate.pem";

const REGISTRY_FILE: &'static str = "registry.pem";

const TMP_FILE_SUFFIX: &'static str = ".tmp";

/// After detecting a file system change, the amount of time the credentials
/// loader waits before actually reading in the new files. This is done to
/// consolidate multiple writes done in a short amount of time.
const LOADER_BATCHING_DELAY: Duration = Duration::from_millis(200);

#[derive(Clone)]
pub struct Credentials {
    pub client: ClientOptionsContainer,
    pub server: ServerOptionsContainer,
}

/// Manages writes to a directory (initially empty) contianing TLS credentials.
///
/// One 'FileCredentialsManager' instance can write to the directory while any
/// number of 'FileCredentialsLoader' instances can read from the directory
/// while it is possibly changing (writes of a new cert+private_key pair or a
/// new registry are atomic).
///
/// WARNING: for efficiency, always write the registry before the first
/// certificate to avoid loading the public roots.
///
/// Internal implementation details:
/// - The private keys are writen based to a path based on their key id so that
///   we only need to perform a atomic replacement operation on the certificate
///   file when replacing both.
///
/// TODO: Support cleaning up old private keys (after a delay to ensure readers
/// don't try reading them).
pub struct FileCredentialsManager {
    dir: LocalPathBuf,
    certificates: Option<Vec<Arc<Certificate>>>,
    registry: Option<Arc<CertificateRegistry>>,
    keys: HashMap<String, Arc<PrivateKey>>,

    // These are kept in sync with the above variables.
    // TODO: We should deferentiate between the readable and writeable versions of this.
    tls_options: Option<(ClientOptionsContainer, ServerOptionsContainer)>,
}

impl FileCredentialsManager {
    /// Loads any existing data present in the given directory and returns a
    /// manager instance that can be used to write the initial/next value of the
    /// credentials.
    ///
    /// NOTE: This assumes that we are the exclusive writer to the directory
    /// (else we need to deal with multiple writers writing to the same tmp
    /// files)
    ///
    /// TODO: Lock the directory
    pub async fn create(dir: &LocalPath) -> Result<Self> {
        let mut registry = None;
        let mut certificates = None;
        let mut keys = HashMap::new();

        for entry in file::read_dir(dir)? {
            if entry.name().ends_with(TMP_FILE_SUFFIX) {
                continue;
            }

            let path = dir.join(entry.name());

            if entry.name() == CERTIFICATE_FILE {
                let data = file::read(path).await?;
                let certs = Certificate::from_pem(data.into())?;
                if certs.is_empty() {
                    return Err(err_msg("Empty certificates file"));
                }

                certificates = Some(certs);
                continue;
            }

            if entry.name() == REGISTRY_FILE {
                let data = file::read(path).await?;
                let value = CertificateRegistry::from_pem(data.into())?;
                if value.certificates().next().is_none() {
                    return Err(err_msg("Empty registry file"));
                }

                registry = Some(Arc::new(value));
                continue;
            }

            if let Some(name) = entry.name().strip_prefix("private_key.") {
                let id = name
                    .strip_suffix(".pem")
                    .ok_or_else(|| err_msg("Invalid private key file name"))?
                    .to_ascii_lowercase();

                let data = file::read(path).await?;
                let value = PrivateKey::from_pem(data.into())?;
                // TODO: Check no duplicates
                keys.insert(id, Arc::new(value));
                continue;
            }
        }

        let mut tls_options = None;
        if let Some(certs) = &certificates {
            let cert_key_id =
                base_radix::hex_encode(certs[0].subject_key_id()).to_ascii_lowercase();

            let cert_key = keys
                .get(&cert_key_id)
                .ok_or_else(|| err_msg("Missing key for certificate"))?;

            let (c, s) =
                Self::create_tls_options(&certs[..], cert_key.clone(), registry.clone()).await?;
            tls_options = Some((c.into(), s.into()));
        }

        Ok(Self {
            dir: dir.to_owned(),
            certificates,
            registry,
            keys,
            tls_options,
        })
    }

    /// Returns non-None if there is a custom registry has been loaded into the
    /// directory.
    pub fn registry(&self) -> Option<Arc<CertificateRegistry>> {
        self.registry.clone()
    }

    pub async fn write_registry(&mut self, registry: Arc<CertificateRegistry>) -> Result<()> {
        if registry.certificates().next().is_none() {
            return Err(err_msg("Not writing an empty registry"));
        }

        let value = registry.to_pem();
        let path = self.dir.join(REGISTRY_FILE);

        Self::atomic_write(&path, value.as_bytes()).await?;

        self.registry = Some(registry);
        self.update_tls_options().await?;

        Ok(())
    }

    /// NOTE: For a certificate to exist, its private key must also exist.
    pub fn certificates(&self) -> Option<&[Arc<Certificate>]> {
        self.certificates.as_ref().map(|v| v.as_ref())
    }

    pub fn certificates_with_private_key(&self) -> Option<(&[Arc<Certificate>], &Arc<PrivateKey>)> {
        if let Some(certs) = &self.certificates {
            let key_id = base_radix::hex_encode(certs[0].subject_key_id()).to_ascii_lowercase();
            let key = self.keys.get(&key_id).unwrap();
            Some((&certs, key))
        } else {
            None
        }
    }

    pub async fn write_certificates(
        &mut self,
        certificates: &[Arc<Certificate>],
        private_key: Arc<PrivateKey>,
    ) -> Result<()> {
        if certificates.is_empty() {
            return Err(err_msg("Empty certificates list provided"));
        }

        let key_id = base_radix::hex_encode(certificates[0].subject_key_id()).to_ascii_lowercase();
        if !self.keys.contains_key(&key_id) {
            let key_data = private_key.to_pem();
            let key_path = self.dir.join(format!("private_key.{}.pem", key_id));

            Self::atomic_write(&key_path, key_data.as_bytes()).await?;

            self.keys.insert(key_id, private_key.clone());
        }

        let path = self.dir.join(CERTIFICATE_FILE);
        let value = Certificate::to_pem(certificates);

        Self::atomic_write(&path, value.as_bytes()).await?;

        self.certificates = Some(certificates.to_vec());
        self.update_tls_options().await?;
        Ok(())
    }

    /// Perform an atomic operation to replace the contents of the file at
    /// 'path' with those in 'data'.
    async fn atomic_write(path: &LocalPath, data: &[u8]) -> Result<()> {
        let mut tmp_path = path.to_owned();
        tmp_path.set_file_name(&format!("{}.tmp", path.file_name().unwrap()));
        file::write(&tmp_path, data).await?;

        // https://man7.org/linux/man-pages/man2/rename.2.html
        // TODO: Directly reference the syscall to ensure this is atomic.
        file::rename(tmp_path, path).await?;

        Ok(())
    }

    pub fn server_options(&self) -> Option<ServerOptionsContainer> {
        self.tls_options.as_ref().map(|(_, s)| s.clone())
    }

    pub fn client_options(&self) -> Option<ClientOptionsContainer> {
        self.tls_options.as_ref().map(|(c, _)| c.clone())
    }

    async fn update_tls_options(&mut self) -> Result<()> {
        let certs = match self.certificates.as_ref() {
            Some(v) => v,
            None => return Ok(()),
        };

        let private_key = {
            let key_id = base_radix::hex_encode(certs[0].subject_key_id()).to_ascii_lowercase();
            self.keys.get(&key_id).unwrap().clone()
        };

        let (c, s) = Self::create_tls_options(certs, private_key, self.registry.clone()).await?;

        if let Some(v) = &self.tls_options {
            v.0.set(c.into());
            v.1.set(s.into());
        } else {
            self.tls_options = Some((c.into(), s.into()));
        }

        Ok(())
    }

    async fn create_tls_options(
        certificates: &[Arc<Certificate>],
        private_key: Arc<PrivateKey>,
        registry: Option<Arc<CertificateRegistry>>,
    ) -> Result<(ClientOptions, ServerOptions)> {
        // TODO: Don't reload the registry if it hasn't changed (since this is usually
        // fairly big)
        let registry = {
            if let Some(r) = registry {
                r
            } else {
                // TODO: Should cache this.
                Arc::new(CertificateRegistry::public_roots().await?)
            }
        };

        // TODO: Block writing to swap.
        let mut server_options =
            ServerOptions::recommended_with(certificates.to_vec(), private_key);
        server_options.certificate_request = Some(CertificateRequestOptions {
            root_certificate_registry: CertificateRegistrySource::Custom(registry.clone()),
            trust_remote_certificate: false,
        });

        let mut client_options = ClientOptions::recommended();
        client_options.certificate_request.root_certificate_registry =
            CertificateRegistrySource::Custom(registry.clone());
        client_options.certificate_auth = Some(server_options.certificate_auth.clone());

        Ok((client_options, server_options))
    }
}

/// Loads TLS server/client credentials from a directory containing PEM files.
///
/// This reads files previously written by a 'FileCredentialsManager' instance
/// and continously monitors the directory for future writes. Upon observing a
/// write, this will automatically reload the in-memory state.
///
/// Specifically this internally maintains:
/// - 1 x509 certificate chain (1 main certificate and a series of intermediates
///   up to a root ca).
/// - 1 private key (corresponding to the above certificate)
/// - 1 x509 root certificate registry (for N certificates)
///
/// NOTE: The Client/ServerOptionsContainers will stop getting updated if this
/// loader is dropped.
pub struct FileCredentialsLoader {
    task: TaskResource,
    shared: Arc<Shared>,
}

struct Shared {
    dir: LocalPathBuf,
    server_options: ServerOptionsContainer,
    client_options: ClientOptionsContainer,
}

impl_resource_passthrough!(FileCredentialsLoader, task);

impl FileCredentialsLoader {
    /// Loads credentials once and optionally starts watching for credential
    /// changes in the background.
    pub async fn create(dir: &LocalPath) -> Result<Self> {
        let mut watcher = Watcher {
            // TODO: Move error remapping closer to the syscall.
            inner: LocalFileWatcher::create()
                .map_err(|e| format_err!("While opening file watcher: {}", e))?,
            mtimes: HashMap::new(),
        };

        // NOTE: We need to watch the directory and not individual files since the
        // renames will make references to the files obsolete.
        watcher.inner.mark(dir)?;

        let (client_options, server_options) = Self::load_once(dir, &mut watcher).await?;

        let shared = Arc::new(Shared {
            dir: dir.to_owned(),
            server_options: server_options.into(),
            client_options: client_options.into(),
        });

        let task: TaskResource = TaskResource::spawn_interruptable(
            "FileCredentialsLoader",
            Self::continously_reload(shared.clone(), watcher),
        );

        Ok(Self { task, shared })
    }

    pub fn certificate(&self) -> Arc<Certificate> {
        self.server_options().get().certificate_auth.certificates[0].clone()
    }

    pub fn private_key(&self) -> Arc<PrivateKey> {
        self.server_options()
            .get()
            .certificate_auth
            .private_key
            .clone()
    }

    pub fn registry(&self) -> Arc<CertificateRegistry> {
        let client_options = self.client_options().get();

        match &client_options.certificate_request.root_certificate_registry {
            CertificateRegistrySource::Custom(v) => v.clone(),
            // We always materialize to ::Custom during the loading process so we should never see
            // this.
            CertificateRegistrySource::PublicRoots => panic!(),
        }
    }

    async fn continously_reload(shared: Arc<Shared>, mut watcher: Watcher) -> Result<()> {
        loop {
            watcher.wait().await?;

            let (c, s) = Self::load_once(&shared.dir, &mut watcher).await?;
            shared.client_options.set(Arc::new(c));
            shared.server_options.set(Arc::new(s));
        }

        Ok(())
    }

    async fn load_once(
        dir: &LocalPath,
        watcher: &mut Watcher,
    ) -> Result<(ClientOptions, ServerOptions)> {
        let registry_path = dir.join(REGISTRY_FILE);
        let cert_path = dir.join(CERTIFICATE_FILE);

        watcher.track(&registry_path).await?;
        watcher.track(&cert_path).await?;

        // TODO: Don't reload the registry if it hasn't changed (since this is usually
        // fairly big)
        let registry = {
            if file::exists(&registry_path).await? {
                Some(Arc::new(CertificateRegistry::from_pem(
                    file::read(&registry_path).await?.into(),
                )?))
            } else {
                None
            }
        };

        let cert = {
            let data = file::read(&cert_path).await?;
            Certificate::from_pem(data.into())?
        };

        if cert.is_empty() {
            return Err(err_msg("Empty certificates set was read"));
        }

        let private_key = {
            let key_id = base_radix::hex_encode(cert[0].subject_key_id()).to_ascii_lowercase();
            let key_path = dir.join(format!("private_key.{}.pem", key_id));
            Arc::new(PrivateKey::from_pem(file::read(key_path).await?.into())?)
        };

        FileCredentialsManager::create_tls_options(&cert, private_key, registry).await
    }

    pub fn server_options(&self) -> ServerOptionsContainer {
        self.shared.server_options.clone()
    }

    pub fn client_options(&self) -> ClientOptionsContainer {
        self.shared.client_options.clone()
    }
}

struct Watcher {
    inner: LocalFileWatcher,
    mtimes: HashMap<String, SystemTime>,
}

impl Watcher {
    pub async fn track(&mut self, path: &LocalPath) -> Result<()> {
        let meta = file::metadata(path).await?;
        self.mtimes.insert(path.as_str().into(), meta.modified());

        Ok(())
    }

    pub async fn wait(&mut self) -> Result<()> {
        loop {
            self.inner.wait().await?;
            executor::sleep(LOADER_BATCHING_DELAY).await?;

            let mut changed = false;
            for (path, last_mtime) in &self.mtimes {
                let meta = file::metadata(LocalPath::new(path)).await?;
                if meta.modified() > *last_mtime {
                    changed = true;
                    break;
                }
            }

            if changed {
                return Ok(());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::x509::PrivateKeyType;

    use super::*;

    use file::temp::TempDir;

    struct Creds {
        certs: Vec<Arc<Certificate>>,
        key: Arc<PrivateKey>,
        registry: Arc<CertificateRegistry>,
    }

    async fn make_creds() -> Result<Creds> {
        let key = Arc::new(PrivateKey::generate(PrivateKeyType::ECDSA_SECP256R1).await?);

        let mut csr = crate::x509::CertificateRequestBuilder::default();
        csr.set_common_name("localhost")?;
        let csr = csr.build(&key).await?;

        let cert_data = crate::x509::CertificateBuilder::new(
            csr,
            Duration::from_secs(60 * 60),
            crate::x509::SubjectValue::CopyCSR,
        )?
        .create_ca()
        .build(None, &key)
        .await?;

        let cert = Arc::new(Certificate::read(cert_data.into())?);

        let mut registry = CertificateRegistry::new();
        registry.append(&[cert.clone()], true)?;

        Ok(Creds {
            certs: vec![cert],
            key,
            registry: Arc::new(registry),
        })
    }

    #[testcase]
    async fn works() -> Result<()> {
        let tmp = TempDir::create()?;

        let mut manager = FileCredentialsManager::create(tmp.path()).await?;
        assert!(manager.registry.is_none());
        assert!(manager.certificates_with_private_key().is_none());

        let creds1 = make_creds().await?;
        manager.write_registry(creds1.registry.clone()).await?;
        manager
            .write_certificates(&creds1.certs, creds1.key.clone())
            .await?;

        let loader = FileCredentialsLoader::create(tmp.path()).await?;
        assert_eq!(loader.private_key().to_der(), creds1.key.to_der());
        assert_eq!(loader.certificate().to_der(), creds1.certs[0].to_der());
        assert_eq!(loader.registry().to_pem(), creds1.registry.to_pem());

        let creds2 = make_creds().await?;
        manager.write_registry(creds2.registry.clone()).await?;

        executor::sleep(Duration::from_secs(2)).await?;
        assert_eq!(loader.private_key().to_der(), creds1.key.to_der());
        assert_eq!(loader.certificate().to_der(), creds1.certs[0].to_der());
        assert_eq!(loader.registry().to_pem(), creds2.registry.to_pem());
        assert!(loader.registry().to_pem() != creds1.registry.to_pem());

        manager
            .write_certificates(&creds2.certs, creds2.key.clone())
            .await?;

        executor::sleep(Duration::from_secs(2)).await?;
        assert_eq!(loader.private_key().to_der(), creds2.key.to_der());
        assert_eq!(loader.certificate().to_der(), creds2.certs[0].to_der());
        assert_eq!(loader.registry().to_pem(), creds2.registry.to_pem());

        // What to verify that the loader can handle multiple changes to the same
        // underlying path.

        let creds3 = make_creds().await?;
        manager.write_registry(creds3.registry.clone()).await?;
        manager
            .write_certificates(&creds3.certs, creds3.key.clone())
            .await?;

        executor::sleep(Duration::from_secs(2)).await?;
        assert_eq!(loader.private_key().to_der(), creds3.key.to_der());
        assert_eq!(loader.certificate().to_der(), creds3.certs[0].to_der());
        assert_eq!(loader.registry().to_pem(), creds3.registry.to_pem());

        Ok(())
    }
}
