use alloc::boxed::Box;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::time::Duration;
use std::borrow::ToOwned;
use std::collections::{HashMap, BTreeMap, HashSet};
use std::sync::Arc;
use std::time::{SystemTime, Instant};

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

const GARBAGE_COLLECTION_DELAY: Duration = Duration::from_secs(2);

#[derive(Clone)]
pub struct Credentials {
    pub client: ClientOptionsContainer,
    pub server: ServerOptionsContainer,
}

/// Manages writes to a directory (initially empty) containing TLS credentials.
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
    
    data: CredentialsData,

    // These are kept in sync with the above variables.
    // TODO: We should deferentiate between the readable and writeable versions of this.
    tls_options: Option<(ClientOptionsContainer, ServerOptionsContainer)>,

    last_write: Instant,
}

struct CredentialsData {
    // A BTreeMap is used so that when iterating, we treat the empty name
    // certificate as the primary one.
    certificates: BTreeMap<String, Vec<Arc<Certificate>>>,
    
    registry: Option<Arc<CertificateRegistry>>,

    keys: HashMap<String, Arc<PrivateKey>>,
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
        let data = Self::load_once(dir).await?;

        let mut tls_options = None;
        if !data.certificates.is_empty() {
            let (c, s) = Self::create_tls_options(&data).await?;
            tls_options = Some((c.into(), s.into()));
        }

        Ok(Self {
            dir: dir.to_owned(),
            data,
            tls_options,
            last_write: Instant::now()
        })
    }

    async fn load_once(dir: &LocalPath) -> Result<CredentialsData> {
        let mut registry = None;
        let mut certificates = BTreeMap::new();
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

                certificates.insert("".to_string(), certs);
                continue;
            }

            if let Some(name) = entry.name().strip_prefix("certificate.") {
                let name = match name.strip_suffix(".pem") {
                    Some(v) => v.to_ascii_lowercase(),
                    None => continue 
                };

                let data = file::read(path).await?;
                let certs = Certificate::from_pem(data.into())?;
                if certs.is_empty() {
                    return Err(err_msg("Empty certificates file"));
                }

                certificates.insert(name.to_string(), certs);
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

        Ok(CredentialsData {
            registry,
            certificates,
            keys
        })
    }

    pub fn dir(&self) -> &LocalPath {
        &self.dir
    }

    /// Returns non-None if there is a custom registry has been loaded into the
    /// directory.
    pub fn registry(&self) -> Option<Arc<CertificateRegistry>> {
        self.data.registry.clone()
    }

    pub async fn write_registry(&mut self, registry: Arc<CertificateRegistry>) -> Result<()> {
        if registry.certificates().next().is_none() {
            return Err(err_msg("Not writing an empty registry"));
        }

        let value = registry.to_pem();
        let path = self.dir.join(REGISTRY_FILE);

        Self::atomic_write(&path, value.as_bytes()).await?;

        self.data.registry = Some(registry);
        self.update_tls_options().await?;

        Ok(())
    }

    /// NOTE: For a certificate to exist, its private key must also exist.
    pub fn certificates(&self) -> Option<&[Arc<Certificate>]> {
        self.certificates_with_name("")
    }

    pub fn certificates_with_name(&self, name: &str) -> Option<&[Arc<Certificate>]> {
        self.data.certificates.get(name).map(|v| v.as_ref())
    }

    pub fn certificates_with_private_key(&self) -> Option<(&[Arc<Certificate>], &Arc<PrivateKey>)> {
        if let Some(certs) = self.data.certificates.get("") {
            let key_id = base_radix::hex_encode(certs[0].subject_key_id()).to_ascii_lowercase();
            let key = self.data.keys.get(&key_id).unwrap();
            Some((&certs, key))
        } else {
            None
        }
    }

    /// Gets the raw file system path to a certificate and private key PEM file.
    ///
    /// AVOID USING
    pub fn certificate_and_pkey_path(&self, name: &str) -> Option<(LocalPathBuf, LocalPathBuf)> {
        let certs = match self.data.certificates.get(name) {
            Some(v) => v,
            None => return None
        };

        let path = if name.is_empty() {
            self.dir.join(CERTIFICATE_FILE)
        } else {
            self.dir.join(format!("certificate.{}.pem", name))
        };

        let key_id = base_radix::hex_encode(certs[0].subject_key_id()).to_ascii_lowercase();
        let key = self.dir.join(format!("private_key.{}.pem", key_id));

        Some((path, key))
    }

    pub async fn write_certificates(
        &mut self,
        certificates: &[Arc<Certificate>],
        private_key: Arc<PrivateKey>,
    ) -> Result<()> {
        self.write_certificates_with_name("", certificates, private_key).await
    }

    pub async fn write_certificates_with_name(
        &mut self,
        name: &str,
        certificates: &[Arc<Certificate>],
        private_key: Arc<PrivateKey>,
    ) -> Result<()> {
        if certificates.is_empty() {
            return Err(err_msg("Empty certificates list provided"));
        }

        let key_id = base_radix::hex_encode(certificates[0].subject_key_id()).to_ascii_lowercase();
        if !self.data.keys.contains_key(&key_id) {
            let key_data = private_key.to_pem();
            let key_path = self.dir.join(format!("private_key.{}.pem", key_id));

            Self::atomic_write(&key_path, key_data.as_bytes()).await?;

            self.data.keys.insert(key_id, private_key.clone());
        }

        let path = if name.is_empty() {
            self.dir.join(CERTIFICATE_FILE)
        } else {
            self.dir.join(format!("certificate.{}.pem", name))
        };
        let value = Certificate::to_pem(certificates);

        Self::atomic_write(&path, value.as_bytes()).await?;

        self.data.certificates.insert(name.to_string(), certificates.to_vec());
        self.update_tls_options().await?;

        self.last_write = Instant::now();

        Ok(())
    }

    /// Deletes all keys not referenced by some certificate.
    ///
    /// Call after completing a batch of writes to the credentials.
    pub async fn gc(&mut self) -> Result<()> {
        // Allow some time for clients to read the latest credentials before deleting old files.
        let now = Instant::now();
        if let Some(remaining) = (self.last_write + GARBAGE_COLLECTION_DELAY).checked_duration_since(now) {
            executor::sleep(remaining).await?;
        }

        let mut referenced_keys = HashSet::new();
        for certificates in self.data.certificates.values() {
            let key_id = base_radix::hex_encode(certificates[0].subject_key_id()).to_ascii_lowercase();
            referenced_keys.insert(key_id);
        }

        let mut keys_to_delete = vec![];
        for key in self.data.keys.keys() {
            if !referenced_keys.contains(key) {
                keys_to_delete.push(key.clone());
            }
        }

        for key_id in keys_to_delete {
            self.data.keys.remove(&key_id);

            let key_path = self.dir.join(format!("private_key.{}.pem", key_id));
            file::remove_file(&key_path).await?;
        }

        Ok(())
    }

    /// Perform an atomic operation to replace the contents of the file at
    /// 'path' with those in 'data'.
    async fn atomic_write(path: &LocalPath, data: &[u8]) -> Result<()> {
        #[cfg(target_os = "linux")]
        let original_file_name = path.file_name().unwrap();
        #[cfg(not(target_os = "linux"))]
        let original_file_name = path.file_name().unwrap().to_str().unwrap();
        
        let mut tmp_path = path.to_owned();
        tmp_path.set_file_name(&format!("{}.tmp", original_file_name));
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
        if self.data.certificates.is_empty() {
            return Ok(());
        }

        let (c, s) = Self::create_tls_options(&self.data).await?;

        if let Some(v) = &self.tls_options {
            v.0.set(c.into());
            v.1.set(s.into());
        } else {
            self.tls_options = Some((c.into(), s.into()));
        }

        Ok(())
    }

    async fn create_tls_options(
        data: &CredentialsData
    ) -> Result<(ClientOptions, ServerOptions)> {
        // TODO: Don't reload the registry if it hasn't changed (since this is usually
        // fairly big)
        let registry = {
            if let Some(r) = &data.registry {
                r.clone()
            } else {
                // TODO: Should cache this.
                Arc::new(CertificateRegistry::public_roots().await.map_err(
                    |e| format_err!("While loading public roots: {}", e))?)
            }
        };

        let mut client_identifies = vec![];
        let mut server_identities = vec![];

        let mut first_valid = false;
        for (i, certs) in data.certificates.values().enumerate() {
            let cert_key_id =
                base_radix::hex_encode(certs[0].subject_key_id()).to_ascii_lowercase();

            let cert_key = data.keys
                .get(&cert_key_id)
                .ok_or_else(|| err_msg("Missing key for certificate"))?;

            // TODO: Make this an approximate check. Anything that is about to
            // expire should be invalid.
            if !certs[0].valid_now() {
                eprintln!("[WARNING] Discarding expired certificate for common name \"{}\"",
                    certs[0].subject().common_name()?
                        .unwrap_or_else(|| "<unknown name format>".into()));
                continue;
            }

            let identity = CertificateIdentity {
                certificates: certs.clone(),
                private_key: cert_key.clone()
            };

            // We don't support dynamically selecting client certificates so we always
            // use the first one.
            if i == 0 {
                client_identifies.push(identity.clone());
            }

            server_identities.push(identity);
        }

        // TODO: Block writing to swap.
        let mut server_options =
            ServerOptions::recommended_with_identities(server_identities);
        server_options.certificate_request = Some(CertificateRequestOptions {
            root_certificate_registry: CertificateRegistrySource::Custom(registry.clone()),
            trust_remote_certificate: false,
        });

        let mut client_options = ClientOptions::recommended();
        client_options.certificate_request.root_certificate_registry =
            CertificateRegistrySource::Custom(registry.clone());

        client_options.certificate_auth = CertificateAuthenticationOptions {
            identities: client_identifies
        };

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
/// - N named certificate identifies each consisting of:
///   - 1 x509 certificate chain (1 main certificate and a series of intermediates
///   up to a root ca).
///   - 1 private key (corresponding to the above certificate)
/// - 1 x509 root certificate registry (for M certificates)
///
/// The TLS semantics are to use the certificate identifies in priority of
/// lexicograpic ordering of the names. Only the first identity will be used
/// for client authentication but all will be considered as options for the server
/// identity if a server/host name is sent by the client.
///
/// The typical usage will be to give the dynamic identity a name of "" which is
/// always first in priority.
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
    /// Loads credentials once and starts watching for credential
    /// changes in the background.
    pub async fn create(dir: &LocalPath) -> Result<Self> {
        // TODO: Move error remapping closer to the syscall.
        let mut watcher = LocalFileWatcher::create()
            .map_err(|e| format_err!("While opening file watcher: {}", e))?;

        // NOTE: We need to watch the directory and not individual files since the
        // renames will make references to the files obsolete.
        watcher.mark(dir)
            .map_err(|e| format_err!("While watching directory: {}", e))?;

        let (client_options, server_options) = Self::load_once(dir).await?;

        let shared = Arc::new(Shared {
            dir: dir.to_owned(),
            server_options: server_options.into(),
            client_options: client_options.into(),
        });

        let task = TaskResource::spawn_interruptable(
            "FileCredentialsLoader",
            Self::continously_reload(shared.clone(), watcher),
        );

        Ok(Self { task, shared })
    }

    pub fn certificate(&self) -> Option<Arc<Certificate>> {
        let opts = self.server_options().get();
        if opts.certificate_auth.identities.is_empty() {
            return None;
        }

        Some(opts.certificate_auth.identities[0].certificates[0].clone())
    }

    pub fn private_key(&self) -> Option<Arc<PrivateKey>> {
        let opts = self.server_options().get();
        if opts.certificate_auth.identities.is_empty() {
            return None;
        }

        Some(opts
            .certificate_auth
            .identities[0]
            .private_key
            .clone())
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

    async fn continously_reload(shared: Arc<Shared>, mut watcher: LocalFileWatcher) -> Result<()> {
        loop {
            watcher.wait().await.map_err(|e| format_err!("While waiting for changes: {}", e))?;

            // Capture all events within the batching window after the first event.
            let batch_timeout = Instant::now() + LOADER_BATCHING_DELAY;
            loop {
                let now = Instant::now();
                let remaining = match batch_timeout.checked_duration_since(now) {
                    Some(v) => core::cmp::max(v, Duration::from_millis(2)),
                    None => break
                };

                let res = match executor::timeout(remaining, watcher.wait()).await {
                    Ok(v) => v,
                    Err(_) => {
                        // Timeout
                        break;
                    }
                };

                res?;
            }

            let (c, s) = Self::load_once(&shared.dir).await?;
            shared.client_options.set(Arc::new(c));
            shared.server_options.set(Arc::new(s));
        }

        Ok(())
    }

    async fn load_once(
        dir: &LocalPath,
    ) -> Result<(ClientOptions, ServerOptions)> {
        let data = FileCredentialsManager::load_once(dir).await?;
        FileCredentialsManager::create_tls_options(&data).await
    }

    pub fn server_options(&self) -> ServerOptionsContainer {
        self.shared.server_options.clone()
    }

    pub fn client_options(&self) -> ClientOptionsContainer {
        self.shared.client_options.clone()
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
        assert!(manager.registry().is_none());
        assert!(manager.certificates_with_private_key().is_none());

        let creds1 = make_creds().await?;
        manager.write_registry(creds1.registry.clone()).await?;
        manager
            .write_certificates(&creds1.certs, creds1.key.clone())
            .await?;

        let loader = FileCredentialsLoader::create(tmp.path()).await?;
        assert_eq!(loader.private_key().unwrap().to_der(), creds1.key.to_der());
        assert_eq!(loader.certificate().unwrap().to_der(), creds1.certs[0].to_der());
        assert_eq!(loader.registry().to_pem(), creds1.registry.to_pem());

        let creds2 = make_creds().await?;
        manager.write_registry(creds2.registry.clone()).await?;

        executor::sleep(Duration::from_secs(2)).await?;
        assert_eq!(loader.private_key().unwrap().to_der(), creds1.key.to_der());
        assert_eq!(loader.certificate().unwrap().to_der(), creds1.certs[0].to_der());
        assert_eq!(loader.registry().to_pem(), creds2.registry.to_pem());
        assert!(loader.registry().to_pem() != creds1.registry.to_pem());

        manager
            .write_certificates(&creds2.certs, creds2.key.clone())
            .await?;

        executor::sleep(Duration::from_secs(2)).await?;
        assert_eq!(loader.private_key().unwrap().to_der(), creds2.key.to_der());
        assert_eq!(loader.certificate().unwrap().to_der(), creds2.certs[0].to_der());
        assert_eq!(loader.registry().to_pem(), creds2.registry.to_pem());

        // What to verify that the loader can handle multiple changes to the same
        // underlying path.

        let creds3 = make_creds().await?;
        manager.write_registry(creds3.registry.clone()).await?;
        manager
            .write_certificates(&creds3.certs, creds3.key.clone())
            .await?;

        executor::sleep(Duration::from_secs(2)).await?;
        assert_eq!(loader.private_key().unwrap().to_der(), creds3.key.to_der());
        assert_eq!(loader.certificate().unwrap().to_der(), creds3.certs[0].to_der());
        assert_eq!(loader.registry().to_pem(), creds3.registry.to_pem());

        Ok(())
    }
}
