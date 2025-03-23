use std::{
    collections::HashMap,
    sync::{Arc, Weak},
    time::{Duration, SystemTime},
};

use cluster_client::{
    meta::{client::ClusterMetaClient, CertificateMetadataTable},
    service::{
        address::{ServiceEntity, ServiceName},
        create_rpc_channel,
    },
    CertificateAuthorityStub, CertificateSecrets, SignCertificateRequest,
};
use common::bytes::Bytes;
use common::errors::*;
use crypto::{
    hasher::Hasher,
    sha256::SHA256Hasher,
    tls::FileCredentialsManager,
    x509::{Certificate, CertificateRegistry, PrivateKey},
};
use db_table::query;
use executor::{
    bundle::{TaskBundle, TaskResultBundle},
    channel,
    sync::{AsyncMutex, Eventually},
};
use executor::{cancellation::CancellationToken, lock};
use executor::{child_task::ChildTask, lock_async};
use executor_multitask::{impl_resource_passthrough, ServiceResourceGroup, TaskResource};
use file::{LocalPath, LocalPathBuf};

/// Called when the credentials for a worker are ready for use.
pub type WorkerCredentialsReadyCallback = Box<dyn Fn(&str) -> () + Send + Sync>;

/// Maintains the state of all node/worker credentials for this cluster node.
///
/// Internal details:
///
/// Certificates are signed by contacting the certificate authority job.
///
/// This is implemented as a set of tasks:
/// - Registry refresh task: polls the metastore for the current state of the
///   certificate registry and writes it to disk when there is a change.
/// - Node credentials task: refreshs the node's credentials when they are close
///   to expiring.
/// - Per-worker task for doing the initial certificate load and refreshing each
///   worker's credentials.
///
/// Note that both the registry and per-worker tasks need to write to the
/// per-worker registry files to keep them in sync with the node's registry. To
/// make this safe, we require having an exclusive lock on the state whenever
/// writing to any registry files.
pub struct NodeCredentialsManager {
    shared: Arc<Shared>,
    task: TaskResource,
}

impl_resource_passthrough!(NodeCredentialsManager, task);

struct Shared {
    node_id: u64,
    zone: String,
    ca: Eventually<CertificateAuthorityStub>,
    meta_client: Arc<Eventually<Arc<ClusterMetaClient>>>,
    callback: WorkerCredentialsReadyCallback,
    node_server_options: crypto::tls::ServerOptionsContainer,
    node_client_options: crypto::tls::ClientOptionsContainer,
    state: AsyncMutex<State>,
    node_event_sender: channel::Sender<()>,
}

struct State {
    /// The newest registry loaded from the metastore.
    ///
    /// This is updated by the registry refresh task and consumed by the
    /// node/per-worker tasks to update the individual credential directories.
    latest_registry: Arc<CertificateRegistry>,

    workers: HashMap<String, WorkerState>,
}

struct WorkerState {
    certificate_expiration: Option<SystemTime>,
    event_sender: channel::Sender<()>,
    injected_credentials: Option<CertificateSecrets>,

    // TODO: This currently contains a cyclic loop to the Arc<Shared> so must be separately cleaned
    // up in our main() function if we want everything to be destroyed correctly.
    task: ChildTask,
}

struct WorkerStoredCredentials {
    loader: FileCredentialsManager,
    registry_hash: Bytes,
}

struct WorkerCredentials {
    dir: LocalPathBuf,
    cert: Arc<Certificate>,
}

impl NodeCredentialsManager {
    pub async fn create(
        node_id: u64,
        zone: &str,
        node_credentials_dir: &LocalPath,
        meta_client: Arc<Eventually<Arc<ClusterMetaClient>>>,
        callback: WorkerCredentialsReadyCallback,
    ) -> Result<Self> {
        // Note that we don't watch the directory since we only support credential
        // reload in cluster mode.
        let node_credentials = FileCredentialsManager::create(node_credentials_dir).await?;
        if node_credentials.certificates().is_none() || node_credentials.registry().is_none() {
            return Err(err_msg("Missing initial node credentials"));
        }

        let latest_registry = node_credentials.registry().unwrap();

        let (node_event_sender, node_event_receiver) = channel::bounded(1);

        let shared = Arc::new(Shared {
            node_id,
            zone: zone.to_string(),
            ca: Eventually::new(),
            meta_client,
            callback,
            node_event_sender,
            node_client_options: node_credentials.client_options().unwrap(),
            node_server_options: node_credentials.server_options().unwrap(),
            state: AsyncMutex::new(State {
                latest_registry,
                workers: HashMap::new(),
            }),
        });

        let resources = ServiceResourceGroup::new("NodeCredentialsManager");

        let task = TaskResource::spawn_interruptable(
            "NodeCredentialsManager",
            Self::main_task(shared.clone(), node_event_receiver, node_credentials),
        );

        Ok(Self { task, shared })
    }

    pub fn node_server_options(&self) -> crypto::tls::ServerOptionsContainer {
        self.shared.node_server_options.clone()
    }

    pub fn node_client_options(&self) -> crypto::tls::ClientOptionsContainer {
        self.shared.node_client_options.clone()
    }

    /// Adds a worker to the pool of workers for which credentials are being
    /// tracked.
    ///
    /// - Credentials for this specific worker will be read/written into
    ///   'credentials_dir'.
    ///   - Note that all calls to this for the same worker must specify the
    ///     same worker_name
    /// - This function does not block for loading to be complete as that is all
    ///   done asyncrously.
    /// -
    /// - If the worker has already been registered, this does nothing.
    ///
    /// Returns whether or not the credentials for this worker are already ready
    /// for usage. 'Ready' means that all the credentials have been written into
    /// 'credentials_dir' and they are valid/unexpired.
    pub async fn add_worker(
        &self,
        worker_name: &str,
        credentials_dir: &LocalPath,
        injected_credentials: Option<CertificateSecrets>,
    ) -> Result<bool> {
        //

        lock!(state <= self.shared.state.lock().await?, {
            if let Some(worker) = state.workers.get_mut(worker_name) {
                if let Some(v) = injected_credentials {
                    worker.injected_credentials = Some(v);
                    let _ = worker.event_sender.try_send(());
                }

                if let Some(expiration_time) = &worker.certificate_expiration {
                    if expiration_time
                        .duration_since(SystemTime::now())
                        .unwrap_or(Duration::ZERO)
                        >= cluster_client::credentials::WORKER_CERT_MIN_REMAINING
                    {
                        return Ok(true);
                    }
                }

                return Ok(false);
            }

            let (event_sender, event_receiver) = channel::bounded(1);

            // TODO: Maybe add to the main resource group/support graceful cancellation?
            let task = ChildTask::spawn(Self::worker_updater_task(
                self.shared.clone(),
                worker_name.to_string(),
                credentials_dir.to_owned(),
                event_receiver,
            ));

            state.workers.insert(
                worker_name.to_string(),
                WorkerState {
                    event_sender,
                    certificate_expiration: None,
                    injected_credentials,
                    task,
                },
            );

            Ok(false)
        })
    }

    /// Stops tracking the credentials for the given worker.
    ///
    /// Any credential files will remain on disk, but will not be further
    /// refreshed.
    pub async fn remove_worker(&self, worker_name: &str) -> Result<()> {
        let task = lock!(state <= self.shared.state.lock().await?, {
            state.workers.remove(worker_name).map(|e| e.task)
        });

        // Wait for the task to finish so that future calls to add_worker don't risk
        // having two tasks running at the same time.
        //
        // TODO: Ideally have a more robust mechanism since add_worker may be called in
        // parallel to remove_worker.
        if let Some(task) = task {
            task.cancel().await;
        }

        Ok(())
    }

    async fn main_task(
        shared: Arc<Shared>,
        node_event_receiver: channel::Receiver<()>,
        node_credendials: FileCredentialsManager,
    ) -> Result<()> {
        // NOTE: This will never return if the node is not in a cluster zone.
        let meta_client = shared.meta_client.get().await;

        // Note that channel creation should never fail unless the address is invalid.
        let channel = create_rpc_channel(
            "cert-authority.system.job.local.cluster.internal",
            meta_client.clone(),
        )
        .await?;

        shared
            .ca
            .set(CertificateAuthorityStub::new(channel))
            .await?;

        let mut bundle = TaskBundle::new();
        bundle.add(Self::registry_updater_task(
            shared.clone(),
            meta_client.clone(),
        ));
        bundle.add(Self::node_updater_task(
            shared.clone(),
            node_event_receiver,
            node_credendials,
        ));

        bundle.join().await;

        Ok(())
    }

    async fn registry_updater_task(shared: Arc<Shared>, meta_client: Arc<ClusterMetaClient>) {
        loop {
            if let Err(e) = Self::registry_updater_task_inner(&shared, &meta_client).await {
                eprintln!("[Node Registry Updater] {}", e);
            }

            executor::sleep(Duration::from_secs(60 * 5)).await; // 5 minutes
        }
    }

    async fn registry_updater_task_inner(
        shared: &Shared,
        meta_client: &ClusterMetaClient,
    ) -> Result<()> {
        /*
        TODO: Assumption is that the registries internally can handle expired certificates (We shouldn't need to deal with it at this layer)
        */

        let db = meta_client.db();

        let mut last_registry = lock!(state <= shared.state.lock().await?, {
            state.latest_registry.clone()
        });

        loop {
            // TODO: Ideally this would be handled more like a generic 'data push' that
            // gradually rolls out to more and more nodes with each reported health of
            // consuming the data.
            let certs = query!(db, CertificateMetadataTable, "root = true");

            if certs.len() == 0 {
                return Err(err_msg("Unable to find any root certificates"));
            }

            let mut new_registry = CertificateRegistry::new();
            for cert in certs {
                let c = Certificate::read(cert.data().into())?;
                new_registry.append(&[Arc::new(c)], true)?;
            }

            let new_registry = Arc::new(new_registry);
            let new_hash = Self::hash_registry(&new_registry)?;

            let last_hash = Self::hash_registry(&last_registry)?;

            if last_hash != new_hash {
                // Update the registry and notify other tasks to take a look.

                lock!(state <= shared.state.lock().await?, {
                    state.latest_registry = new_registry.clone();

                    for worker in state.workers.values() {
                        let _ = worker.event_sender.try_send(());
                    }
                });

                let _ = shared.node_event_sender.try_send(());

                last_registry = new_registry;
            }

            // 2 hours.
            // TODO: Needs to be dynamic to allow quickly pulling in CA changes.
            // TODO: Switch to 'watching' the db.
            executor::sleep(Duration::from_secs(60 * 60 * 2)).await?;
        }
    }

    /// This task will periodically try to
    async fn node_updater_task(
        shared: Arc<Shared>,
        event_receiver: channel::Receiver<()>,
        mut creds: FileCredentialsManager,
    ) {
        loop {
            if let Err(e) =
                Self::node_updater_task_inner(&shared, &event_receiver, &mut creds).await
            {
                eprintln!("[Node Credential Refresher] Failed: {}", e);
            }

            executor::timeout(Duration::from_secs(60 * 5), event_receiver.recv()).await;
            // 5 minutes
        }
    }

    async fn node_updater_task_inner(
        shared: &Shared,
        event_receiver: &channel::Receiver<()>,
        creds: &mut FileCredentialsManager,
    ) -> Result<()> {
        let meta_client = shared.meta_client.get().await;
        let node_name = ServiceName::for_node(meta_client.zone(), shared.node_id)?;

        loop {
            Self::update_registry_once(shared, creds).await?;
            Self::update_credentials_once(shared, &node_name, creds).await?;

            // TODO: Allow cancellation.
            // 2 hours
            executor::timeout(Duration::from_secs(60 * 60 * 2), event_receiver.recv()).await;
        }
    }

    /// NOTE: This must be able to complete one full inner iteration of
    /// 'worker_updater_task_inner' without depending on the meta_client when
    /// there are already credentials available on disk.
    async fn worker_updater_task(
        shared: Arc<Shared>,
        worker_name: String,
        dir: LocalPathBuf,
        event_receiver: channel::Receiver<()>,
    ) {
        loop {
            if let Err(e) =
                Self::worker_updater_task_inner(&shared, &worker_name, &dir, &event_receiver).await
            {
                eprintln!("[Node Worker Updater] Failed: {}", e);
            }

            executor::sleep(Duration::from_secs(60 * 5)).await; // 5 minutes
        }
    }

    async fn worker_updater_task_inner(
        shared: &Shared,
        worker_name: &str,
        dir: &LocalPath,
        event_receiver: &channel::Receiver<()>,
    ) -> Result<()> {
        let mut creds = FileCredentialsManager::create(dir).await?;

        let service_name = ServiceName::for_worker(&shared.zone, worker_name)?;

        // Setting to true if we have certificates in order to perform the initial
        // update.
        let mut cert_changed = creds.certificates().is_some();

        loop {
            Self::update_registry_once(&shared, &mut creds).await?;

            // Read in any one off injected credentials.
            {
                let injected_creds = lock!(state <= shared.state.lock().await?, {
                    state
                        .workers
                        .get_mut(worker_name)
                        .and_then(|worker| worker.injected_credentials.take())
                });

                if let Some(c) = injected_creds {
                    let private_key =
                        Arc::new(crypto::x509::PrivateKey::from_der(c.private_key().into())?);

                    let mut certs = vec![];
                    for data in c.certificates() {
                        certs.push(Arc::new(crypto::x509::Certificate::read(
                            data.as_ref().into(),
                        )?));
                    }

                    creds.write_certificates(&certs, private_key).await?;
                    cert_changed = true;
                }
            }

            // TODO: Allow this to successfully 'time out' just the 'generate_credentials'
            // part of this with some fast follow up.
            cert_changed |=
                Self::update_credentials_once(&shared, &service_name, &mut creds).await?;

            if cert_changed {
                let not_after =
                    SystemTime::from(creds.certificates().unwrap()[0].validity().not_after);
                lock!(state <= shared.state.lock().await?, {
                    if let Some(worker) = state.workers.get_mut(worker_name) {
                        worker.certificate_expiration = Some(not_after);
                    }
                });

                (shared.callback)(worker_name);
                cert_changed = false;
            }

            // 2 hours
            executor::timeout(Duration::from_secs(60 * 60 * 2), event_receiver.recv()).await;
        }
    }

    async fn update_registry_once(
        shared: &Shared,
        creds: &mut FileCredentialsManager,
    ) -> Result<()> {
        let latest_registry = lock!(state <= shared.state.lock().await?, {
            state.latest_registry.clone()
        });

        let registry_stale = {
            if let Some(last_registry) = creds.registry() {
                let latest_hash = Self::hash_registry(&latest_registry)?;
                let last_hash = Self::hash_registry(&last_registry)?;
                latest_hash != last_hash
            } else {
                true
            }
        };

        if registry_stale {
            creds.write_registry(latest_registry.clone()).await?;
        }

        Ok(())
    }

    /// If this returns successfully, then 'creds' should contain a complete
    /// up-to-date valid credentials set and the given entity.
    async fn update_credentials_once(
        shared: &Shared,
        name: &ServiceName,
        creds: &mut FileCredentialsManager,
    ) -> Result<bool> {
        let cert_stale = {
            if let Some(certs) = creds.certificates() {
                let now = SystemTime::now();
                let not_after = SystemTime::from(certs[0].validity().not_after);
                let time_remaining = not_after.duration_since(now).unwrap_or(Duration::ZERO);

                time_remaining
                    <= cluster_client::credentials::cert_refresh_below_duration(&name.entity())
                        .unwrap()
            } else {
                true
            }
        };

        if cert_stale {
            let (new_certs, new_key) = Self::generate_credentials(&shared, &name).await?;
            creds.write_certificates(&new_certs, new_key).await?;
        }

        Ok(cert_stale)
    }

    async fn generate_credentials(
        shared: &Shared,
        name: &ServiceName,
    ) -> Result<(Vec<Arc<Certificate>>, Arc<PrivateKey>)> {
        let ca = shared.ca.get().await;

        let private_key = Arc::new(
            crypto::x509::PrivateKey::generate(crypto::x509::PrivateKeyType::ECDSA_SECP256R1)
                .await?,
        );

        let mut csr = crypto::x509::CertificateRequestBuilder::default();
        csr.set_common_name(&name.to_string())?;

        let csr = csr.build(&private_key).await?;

        let mut req = SignCertificateRequest::default();
        req.set_csr(csr.to_der());

        let req_ctx = rpc::ClientRequestContext::default();
        let res = executor::timeout(Duration::from_secs(5), ca.SignCertificate(&req_ctx, &req))
            .await?
            .result?;

        let mut certs = vec![];
        for cert_data in res.certificate() {
            certs.push(Arc::new(Certificate::read(cert_data[..].into())?));
        }

        Ok((certs, private_key))
    }

    fn hash_registry(registry: &CertificateRegistry) -> Result<Bytes> {
        let mut serials = vec![];
        for cert in registry.certificates() {
            // TODO: Inject a fixed point in time.
            if !cert.valid_now() {
                continue;
            }

            serials.push(cert.serial_number().to_be_bytes());
        }

        serials.sort();

        let mut hasher = SHA256Hasher::default();
        for serial in serials {
            hasher.update(&serial);
        }

        Ok(hasher.finish().into())
    }
}
