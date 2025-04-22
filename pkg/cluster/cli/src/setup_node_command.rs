// Note that code for this command can't depend on having an ambient cluster
// identify (since it must run before cluster PKI has been set up).

/*
cargo run --bin cluster_cli -- setup_node --zone=testing --bootstrap --tls_root=/tmp/tls_root --local_node=/opt/dacha/node

CLUSTER_ZONE=testing CLUSTER_CREDENTIALS=/tmp/tls_root cargo run --bin cluster_cli -- list workers

CLUSTER_ZONE=testing CLUSTER_CREDENTIALS=/tmp/tls_root cargo run --bin cluster_cli -- list nodes

TODO: Safety mesaures needed:
- Must have a well defined local system time before the node can start running.
- Need automatic detection on each RPC of clock syncronization


curl --insecure https://127.0.0.1:10400/profilez > perf.pb

*/

use std::fmt::Debug;
use std::sync::Arc;
use std::time::Duration;

use cluster_ca::*;
use cluster_client::id::{entity_id_to_string, normalize_entity_id};
use cluster_client::ClusterMetaClient;
use cluster_client::meta::*;
use cluster_client::service::create_rpc_channel;
use cluster_client::service::address::{ServiceAddress, ServiceEntity, ServiceName};
use builder::{BuildConfigTarget, Builder};
use common::errors::*;
use cluster_manager::Manager;
use container::NodeConfig;
use container_proto::cluster::*;
use crypto::random::{RngExt, SharedRngExt};
use crypto::tls::{Credentials, FileCredentialsManager};
use db_table::db::ProtobufDB;
use db_table::query_one;
use executor::cancellation::AlreadyCancelledToken;
use executor_multitask::ServiceResource;
use file::temp::TempDir;
use file::{project_dir, project_path, LocalPath, LocalPathBuf};
use hostname::{ClusterMetaHostnameResolver, ROOT_SERVER_ID};
use protobuf::text::{parse_text_proto, ParseTextProto};
use protobuf::Message;
use raft::log::segmented_log::SegmentedLogOptions;
use raft::proto::Configuration_ServerRole;

use crate::acl::{authorize_node, bootstrap_acls};
use crate::ssh::*;
use crate::start_job_command::start_job_impl;
use crate::system_jobs::*;
use crate::utils::*;
use crate::root_credentials::*;
use crate::create_user_command::{run_create_user_impl, read_stdin_password};
use crate::login_command::login_impl;
use crate::nss::check_have_nss_utils;

/// Name of the user on the node's Linux system which will own all main files
/// like node binaries.
const ASSET_OWNER: &'static str = "cluster-user";

/// Name of the user on the node's Linux system which will execute the node
/// binary.
const NODE_USER: &'static str = "cluster-node";

/// List of all groups which (if they exist on the Linux machine) will be allowed
/// to be delegated through the container runtime for containers to access.
const MANAGED_GROUPS: &'static [&'static str] = &[
    "gpio", "plugdev", "dialout", "i2c", "spi", "video", "audio", "edisk",
];

// TODO: Support parsing "\\n" in a regexp?
// TODO: Support specifying that the pattern must start at the beginning of the
// line
// TODO: Make case insensitive.
regexp!(LSCPU_ARCHITECTURE => "(?:^|\n)Architecture:\\s+([^\n]+)\n");
regexp!(CPUINFO_MODEL => "(?:^|\n)Model\\s+:\\s+([^\n]+)\n");

#[derive(Args)]
pub struct SetupNodeCommand {
    zone: String,

    /// If true, this is the first node in the cluster zone so we should
    #[arg(default = false)]
    bootstrap: bool,

    /// Directory in which the root CA private key and TLS certificate are
    /// located.
    ///
    /// If not provided, then we assume that this is available in 
    /// '$HOME/.dacha/credentials/root/[zone]'
    ///
    /// When bootstrap=true, a new key/certificate will be written here.
    tls_root: Option<LocalPathBuf>,

    /// Name of the first user to create in the cluster.
    ///
    /// Only relevant is --bootstrap=true. This first user will be set up as an admin
    /// and the local Linux user will be auto-logged in as this user.
    ///
    /// REQUIRED if 'bootstrap'
    first_user_name: Option<String>,

    /// IP Address of the node machine to setup. This machine needs to be
    /// accessible via SSH.
    ///
    /// Either 'node_addr' or 'local_node' must be specified.
    node_addr: Option<String>,

    /// If true, initialize the local machine as a node. This sets up a single
    /// system wide node running as a systemd service similarly to what is done
    /// by node_addr. 
    ///
    /// Either 'local_node' or 'node_addr' must be specified.
    #[arg(default = false)]
    local_node: bool,

    /// Path on the node machine used to store all node configs, data, and binaries.
    #[arg(default = "/opt/dacha/node")]
    base_dir: String,

    /// If true (default), enable the service so that it starts up automatically on
    /// system restarts.
    #[arg(default = true)]
    enable_service: bool,

    /// Extra argumetns to pass to SSH. This is only relevant when using 'node_addr'.
    ///
    /// NOTE: Arguments with spaces in with are currently not supported.
    #[arg(default = "")]
    ssh_args: String,

    /// For the purposes of initializing the cluster, a local metastore instance
    /// will be brought up before one is running in the cluster.
    ///
    /// This will be the port used on the local machine to server requests to
    /// this instance.
    ///
    /// Not used if 'bootstrap' is false
    #[arg(default = 4000)]
    local_metastore_port: u16,
}

/// TODO: Improve this so that we can continue running it if a previous run
/// failed (mainly needed for the non-bootstrapping case)
pub async fn run_setup_node(cmd: SetupNodeCommand) -> Result<()> {
    if !cluster_client::service::zone::is_valid_zone(&cmd.zone) {
        return Err(format_err!("Invalid --zone argument provided with value: {}", cmd.zone));
    }

    check_have_nss_utils().await?;

    println!("Zone: {}", cmd.zone);
    println!("Bootstrap: {:?}", cmd.bootstrap);

    let root_creds_dir = get_root_credentials_dir(&cmd.zone, &cmd.tls_root)?;
    let root_creds =
        load_or_create_root_credentials(&root_creds_dir, &cmd.zone, cmd.bootstrap).await?;

    let mut first_user_password = None;
    if cmd.bootstrap {
        let user_name = cmd.first_user_name.as_ref()
            .ok_or_else(|| err_msg("--first_user_name must be provided when bootstrapping"))?;

        println!("First User Name: {}", user_name);
        println!("Enter a first user password:");
        first_user_password = Some(read_stdin_password(true).await?);
    }

    // Give a few seconds for the user to cancel if something looks wrong.
    println!("Starting...");
    executor::sleep(Duration::from_secs(5)).await?;

    // When cluster bootstrapping, we need to run a standalone metastore replica
    // until the node can run it by itself.
    let mut local_metastore_resource = None;
    if cmd.bootstrap {
        local_metastore_resource = Some(
            run_local_metastore(
                cmd.local_metastore_port,
                cmd.zone.clone(),
                Some(root_creds.tls.clone()),
            )
            .await?,
        );

        // Wait for the local server to become the leader.
        // TODO: Get rid of this.
        executor::sleep(Duration::from_secs(2)).await?;
    }

    // TODO: Given that we know the port of the local metastore, we can use that to
    // help find it.
    let meta_client = Arc::new(
        ClusterMetaClient::create(
            &cmd.zone,
            &[],
            // TODO: TLS here needs to use the root credentials since we may not yet regular
            // credentials.
            Some(root_creds.tls.clone()),
        )
        .await?,
    );
    let db = meta_client.db();

    if cmd.bootstrap {
        // TODO: Make this all one transaction.

        // TODO: Need to support refreshing this.
        cluster_ca::insert_certificate_into_registry(
            meta_client.db().as_ref(),
            &root_creds.certificate,
            0,
        )
        .await?;

        let mut key_meta = PrivateKeyMetadata::default();
        key_meta.set_id(root_creds.certificate.subject_key_id());
        key_meta.set_data(root_creds.private_key.to_der());
        meta_client
            .db()
            .insert::<PrivateKeyMetadataTable>(&key_meta)
            .await?;

        bootstrap_acls(&cmd.zone, &db).await?;
    }

    let operator: Box<dyn MachineOperator> = {
        if let Some(node_addr) = &cmd.node_addr {
            if cmd.ssh_args.contains("\"") || cmd.ssh_args.contains("'") || cmd.ssh_args.contains("\\") {
                return Err(err_msg("Parsing escaped ssh_args is not supported"));
            }

            let args = cmd.ssh_args.split_whitespace().filter_map(|v| {
                let v = v.trim();
                if v.is_empty() {
                    return None;
                }
                
                Some(v.to_string())
            }).collect::<Vec<_>>();

            Box::new(SSHClient::new(
                node_addr.as_str(),
                ASSET_OWNER,
                args,
            ))
        } else {
            Box::new(LocalOperator::default())
        }
    };

    let node_dir = LocalPath::new(&cmd.base_dir);

    let node_id = {
        let hex = operator.download_string("/etc/machine-id").await?;
        println!("Node Machine Id: {}", hex);

        let data = base_radix::hex_decode(hex.trim())?;
        let id = normalize_entity_id(u64::from_be_bytes(*array_ref![data, 0, 8]));

        id
    };

    println!("Node Id: {}", entity_id_to_string(node_id).unwrap());

    // Verifying that we aren't re-using an existing node id (unless it was used for this same machine).
    // TODO: Acquire a metastore lock for setting up new nodes to avoid concurrent setups with conflicting ids.
    {
        let config_path = node_dir.join("config.pb");
        if operator.file_exists(&config_path).await? {
            let old_node_config = {
                let data = operator.download(&config_path).await?;
                let mut config = NodeConfig::default();
                config.parse_merge(&data)?;
                config
            };

            if old_node_config.zone() != cmd.zone {
                return Err(format_err!("Node already configured for a different zone: {}", old_node_config.zone()));
            }

            if old_node_config.id() != node_id {
                return Err(format_err!("Node already configured with a different id: {}", old_node_config.id()));
            }
        } else {
            let existing_meta = query_one!(db, NodeMetadataTable, "id = ?", node_id);
            if existing_meta.is_some() {
                return Err(err_msg("Node already exists with this id. /etc/machine-id probably wasn't randomly initialized."));
            }
        }
    }

    authorize_node(node_id, &cmd.zone, &db).await?;

    println!("Setting up node runtime:");
    let node_config = {
        let remote = cmd.node_addr.is_some();

        let node_user = {
            if cmd.node_addr.is_some() {
                ASSET_OWNER.to_string()
            } else if cmd.local_node {
                std::env::var("USER")?
            } else {
                todo!()
            }
        };

        setup_remote_node_server(
            operator.as_ref(),
            node_dir,
            &cmd.zone,
            db.clone(),
            &root_creds,
            node_id,
            &node_user,
            remote,
            cmd.bootstrap,
            cmd.enable_service,
        )
        .await?
    };

    // Note that when bootstrapping, this is required so that the manager can
    // schedule the metastore worker immediately without retrying.
    //
    // TODO: This currently doesn't work if we try to restart the only node in a single node cluster
    // (that runs the metastore).
    println!("Waiting for node to register itself:");
    loop {
        if let Some(_) = query_one!(db, NodeMetadataTable, "id = ?", node_id) {
            break;
        }

        executor::sleep(std::time::Duration::from_secs(1)).await;
    }
    println!("=> Done!");

    if cmd.bootstrap {
        let request_context = rpc::ClientRequestContext::default();

        let node = connect_to_node_id(meta_client.clone(), node_id).await?;

        bootstrap_system_jobs(
            node,
            meta_client.clone(),
            db.clone(),
            request_context,
            &cmd.zone,
            node_id,
            &root_creds,
            local_metastore_resource.unwrap(),
        )
        .await?;


        println!("Creating first user...");
        let user_name = cmd.first_user_name.unwrap();
        let user_password = first_user_password.unwrap();

        run_create_user_impl(meta_client.clone(), &user_name, &user_password, &[
            "cluster-readers".into(),
            "cluster-admins".into()
        ]).await?;
    

        // TODO: Use an unauthenticated meta_client.
        println!("Logging in to the user...");
        login_impl(meta_client.clone(), &user_name, &user_password).await?;
    }

    //

    /*

    - For the first node
        - Prerequisite assumption is that we can already securely connect via SSH to the node
        - Upload node runtime binary/config
        - Create + upload initial TLS certificate, registry, and key
            - For the registry, maybe pull from the metastore if that is already setup
        - Start running the node
    - Node will register itself in the metastore
    - Create the CA job
        - It will assign to the first node
        - The node will fail to be unable to start it (because the cert-authority job doesn't exist).
    - Manually add the CA worker to the node with a worker certificate
    - CA worker will locally init the private key certificate and put a CertificateSigningRequest in the metastore
        - This needs to be signed
        - Serial number will be generated locally and used as the id.
    - The local script will accept the CA cert and sign it with the root.
    - CA worker can see that is was accepted once the certificate shows up in the Certificate list

    - From this point on, we just start regular jobs.


    */

    Ok(())
}

// TODO: Replace with the file credentials manager
struct RootCredentials {
    private_key: Arc<crypto::x509::PrivateKey>,
    certificate: Arc<crypto::x509::Certificate>,
    registry: Arc<crypto::x509::CertificateRegistry>,
    tls: crypto::tls::Credentials,
}

async fn load_or_create_root_credentials(
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

async fn run_local_metastore(
    port: u16,
    zone: String,
    tls: Option<crypto::tls::Credentials>,
) -> Result<Arc<dyn ServiceResource>> {
    // TODO: Implement completely in memory.
    let local_metastore_dir = file::temp::TempDir::create()?;

    let res = cluster_meta::run(cluster_meta::ClusterMetastoreOptions {
        id: ROOT_SERVER_ID,
        port,
        zone,
        dir: local_metastore_dir.path().to_owned(),
        creds: tls.unwrap(),
        bootstrap: true,
    })
    .await;

    // Prevent the temp dir from being deleted.
    executor::spawn(async move {
        loop {
            executor::sleep(Duration::from_secs(10)).await;
        }
        drop(local_metastore_dir);
    });

    res
}

struct NodeTLSData {
    certificate: Vec<Arc<crypto::x509::Certificate>>,
    private_key: Arc<crypto::x509::PrivateKey>,
    registry: Arc<crypto::x509::CertificateRegistry>,
}

/// Once this is done, the remote server has a running node runtime.
async fn setup_remote_node_server(
    operator: &dyn MachineOperator,
    base_dir: &LocalPath,
    zone: &str,
    db: Arc<ProtobufDB>,
    root_creds: &RootCredentials,
    node_id: u64,
    asset_owner: &str,
    is_remote: bool,
    bootstrap: bool,
    enable_service: bool,
) -> Result<NodeConfig> {
    check_using_cgroup_v2(operator).await?;

    {
        println!("Stopping old node");
        // This is currently a required step in order to be able to overwrite the in-use
        // files.
        //
        // NOTE: If the service doesn't exist yet, we'll ignore the error.
        operator
            .run("sudo systemctl stop cluster-node | true")
            .await?;
    }

    {
        println!("Setup user: {}", NODE_USER);

        let has_user = operator
            .download_string("/etc/passwd")
            .await?
            .lines()
            .find(|v| v.starts_with(&format!("{}:", NODE_USER)))
            .is_some();

        if has_user {
            println!("=> Already exists")
        } else {
            operator
                .run(&format!(
                    "sudo adduser --system --no-create-home --disabled-password --group {}",
                    NODE_USER
                ))
                .await?;
            println!("=> Newly created!");
        }
    }

    // TODO: Add some unit testing for these against goldens.
    let build_config_target = {
        let lscpu_output = operator.run("lscpu").await?;
        let cpuinfo_output = operator.download_string("/proc/cpuinfo").await?;

        let architecture = LSCPU_ARCHITECTURE
            .exec(&lscpu_output)
            .unwrap()
            .group_str(1)
            .unwrap()?
            .to_string();
        println!("Architecture: {}", architecture);

        let model = match CPUINFO_MODEL.exec(&cpuinfo_output) {
            Some(m) => m.group_str(1).unwrap()?.to_string(),
            None => "".to_string(),
        };

        if architecture == "aarch64" && model.contains("Raspberry Pi") {
            "//pkg/builder/config:rpi64"
        } else if architecture == "x86_64" {
            "//pkg/builder/config:x64"
        } else {
            return Err(format_err!(
                "Unsupported CPU type: {} | {}",
                architecture,
                model
            ));
        }
    };

    println!("Building node runtime with {}", build_config_target);

    let node_built_result = {
        let mut builder = Builder::default()?;

        let result = builder
            .build_target_cwd("//pkg/container:cluster_node_deps", build_config_target)
            .await?;

        result
    };

    /*
    TODO:

        In /etc/hosts, find all entries with 'cluster-node' in them and swap them to to the full one.
        Sample line:

    127.0.1.1               cluster-node

    */

    if is_remote {
        let hostname = format!("cluster-node-{}", entity_id_to_string(node_id).unwrap());

        println!("Setting hostname to: {}", hostname);
        operator
            .run(&format!("sudo hostnamectl set-hostname {}", hostname))
            .await?;
    }

    operator
        .run(&format!("sudo mkdir -p {}", base_dir.as_str()))
        .await?;

    // TODO: Needs to be dynamic for local execution.
    operator
        .run(&format!(
            "sudo chown {owner}:{owner} {base_dir}",
            owner = asset_owner,
            base_dir = base_dir.as_str()
        ))
        .await?;

    // TODO: Setup /opt/dacha/bundle_spec.pb with the description of what was
    // installed

    // Delete any old built artifacts data.
    // TODO: Must use sudo if we are using newcgroup.
    let bundle_dir = base_dir.join("bundle");
    operator
        .run(&format!("rm -rf {}", bundle_dir.as_str()))
        .await?;

    // Cluster cluster data directory
    let data_dir = base_dir.join("data");

    for (key, value) in node_built_result.outputs.output_files {
        let remote_path = bundle_dir.join(key);
        operator
            .create_dir_all(remote_path.parent().unwrap())
            .await?;
        operator.upload_file(value.location, remote_path).await?;
    }

    operator.upload(b"", bundle_dir.join("WORKSPACE")).await?;

    println!("Creating node config...");
    let mut node_config = {
        let s = file::read_to_string(project_path!("pkg/container/config/node.txtpb")).await?;
        NodeConfig::parse_text(&s)?
    };

    node_config.set_id(node_id);
    node_config.set_zone(zone);
    node_config.set_data_dir(data_dir.as_str());
    node_config.set_secure(true);

    let node_config_data = node_config.serialize()?;
    operator
        .upload(&node_config_data, base_dir.join("config.pb"))
        .await?;

    // TODO: This is the only file in the bundle not owned by
    // 'cluster-user:cluster-user'
    // TODO: Find a better place to store this logic that is shared with local
    // executions.
    if !node_config.cgroup_dir().is_empty() {
        let newcgroup_path = bundle_dir.join("built/pkg/container/newcgroup");
        operator
            .run(&format!(
                "sudo chown root:{} {}",
                NODE_USER,
                newcgroup_path.as_str()
            ))
            .await?;
        operator
            .run(&format!("sudo chmod 750 {}", newcgroup_path.as_str()))
            .await?;
        operator
            .run(&format!("sudo chmod u+s {}", newcgroup_path.as_str()))
            .await?;
    }

    // Set up the data directory with the node's TLS certificate.
    if !operator.file_exists(&data_dir).await? {
        println!("Generating initial node credentials...");

        // Creating node TLS identity
        let tls_data = {
            let private_key =
                crypto::x509::PrivateKey::generate(crypto::x509::PrivateKeyType::ECDSA_SECP256R1)
                    .await?;

            let mut csr = crypto::x509::CertificateRequestBuilder::default();
            
            let name = ServiceName::for_node(zone, node_id)?;
            csr.set_common_name(&name.to_string())?;
            let csr = csr.build(&private_key).await?;

            let cert = sign_leaf_certificate(
                &name,
                csr,
                &root_creds.certificate,
                &root_creds.private_key,
            )
            .await?;

            insert_certificate_into_registry(&db, &cert, node_id).await?;

            NodeTLSData {
                certificate: vec![cert],
                private_key: Arc::new(private_key),
                registry: root_creds.registry.clone(),
            }
        };

        println!("Creating node data directory...");
        operator.create_dir_all(&data_dir).await?;

        {
            // TODO: Reference a constant.
            let cert_dir = data_dir.join("credentials/node");
    
            let tmp = TempDir::create()?;
            let mut manager = FileCredentialsManager::create(tmp.path()).await?;
            // Write all the stuff.
            manager.write_registry(tls_data.registry).await?;
            manager
                .write_certificates(&tls_data.certificate, tls_data.private_key)
                .await?;
            drop(manager);
    
            operator.create_dir_all(&cert_dir).await?;
            for entry in file::read_dir(tmp.path())? {
                // TODO: Support uploading sub directories.
                operator
                    .upload_file(tmp.path().join(entry.name()), cert_dir.join(entry.name()))
                    .await?;
            }
        }
    
        // Lock down permissions to the data.
        // (needs to be done after the credentials are all loaded).
        {
            operator
                .run(&format!(
                    "sudo chown -R {owner}:{owner} {dir}",
                    owner = NODE_USER,
                    dir = data_dir.as_str()
                ))
                .await?;
            operator
                .run(&format!("sudo chmod -R 700 {}", data_dir.as_str()))
                .await?;
        }

    } else {
        // We allow skipping during bootstrapping to allow for continuing if a failure occured later on.
        // if bootstrap {
        //     return Err(err_msg("Bootstrapping but the node already has data set up."));
        // }

        println!("Node Data Directory Already Exists! Not re-initializing.");
    }

    // TODO: Don't do this twice if the current file already has the data.
    println!("Setting up /etc/subuid");
    {
        let mut subuid = cleaned_uidmap(&operator.download_string("/etc/subuid").await?, NODE_USER);

        subuid.push_str(&format!("{}:400000:65536", NODE_USER));

        operator
            .upload(subuid.as_bytes(), "/tmp/next_subuid")
            .await?;
        operator.run("sudo cp /tmp/next_subuid /etc/subuid").await?;
    }

    println!("Setting up /etc/subgid");
    {
        let groups_data = operator.download_string("/etc/group").await?;
        let groups = container::node::shadow::read_groups_from_data(&groups_data)?;

        let mut subgid = cleaned_uidmap(&operator.download_string("/etc/subgid").await?, NODE_USER);

        subgid.push_str(&format!("{}:400000:65536\n", NODE_USER));

        for group in groups {
            if MANAGED_GROUPS.iter().find(|g| *g == &group.name).is_some() {
                subgid.push_str(&format!("{}:{}:1\n", NODE_USER, group.id));
            }
        }

        operator
            .upload(subgid.as_bytes(), "/tmp/next_subgid")
            .await?;
        operator.run("sudo cp /tmp/next_subgid /etc/subgid").await?;
    }

    // TODO: Also keep other files like /boot/config.txt in sync

    /*
    TODO: Check if app armor is actually enabled first.

    $ cat /sys/module/apparmor/parameters/enabled
    Y
    */
    // This is a hacky way to check if we are operating in the locked down Ubuntu versions.
    // Sadly it doesn't seem particularly easy to write backwards/forwards compatible profiles.
    if operator.file_exists("/proc/sys/kernel/apparmor_restrict_unprivileged_userns").await? &&
       operator.download_string("/proc/sys/kernel/apparmor_restrict_unprivileged_userns").await?.trim() == "1" {
        println!("Installing apparmor profile...");

        // TODO: Make a single command.

        let profile = file::read_to_string(project_path!("pkg/cluster/config/ubuntu_apparmor")).await?
            .replace("{base_dir}", base_dir.as_str());
        operator.upload(profile.as_bytes(), "/tmp/cluster-apparmor").await?;

        operator.run("sudo cp --no-preserve=all /tmp/cluster-apparmor /etc/apparmor.d/cluster-node").await?;
        operator.run("sudo apparmor_parser -r -W /etc/apparmor.d/cluster-node").await?;
    }

    println!("Installing systemd servoce...");
    let service_file = file::read_to_string(project_path!("pkg/container/config/node.service")).await?
        .replace("{base_dir}", base_dir.as_str());
    operator.upload(service_file.as_bytes(), "/tmp/cluster-node.service").await?;
    operator.run("sudo cp --no-preserve=all /tmp/cluster-node.service /etc/systemd/system/cluster-node.service").await?;

    if enable_service {
        operator.run("sudo systemctl enable cluster-node").await?;
    }

    operator.run("sudo systemctl start cluster-node").await?;

    Ok(node_config)
}

async fn check_using_cgroup_v2(operator: &dyn MachineOperator) -> Result<()> {
    let data = operator.download_string("/proc/cgroups").await?;

    for line in data.lines().skip(1) {
        let fields = line.split_whitespace().collect::<Vec<&str>>();
        if fields[1] != "0" {
            return Err(format_err!("Not using cgroups v2 for '{}' subsystem", fields[0]));
        }
    }


    Ok(())
}

fn cleaned_uidmap(data: &str, remove_user: &str) -> String {
    let remove_prefix = format!("{}:", remove_user);

    let mut out = data
        .lines()
        .filter(|line| !line.starts_with(&remove_prefix))
        .collect::<Vec<_>>()
        .join("\n");
    out.push('\n');
    out
}

// Sets up all the core system jobs to run on the first
async fn bootstrap_system_jobs(
    node: NodeStubs,
    meta_client: Arc<ClusterMetaClient>,
    db: Arc<ProtobufDB>,
    request_context: rpc::ClientRequestContext,
    zone: &str,
    node_id: u64,
    root_creds: &RootCredentials,
    local_metastore_resource: Arc<dyn ServiceResource>,
) -> Result<()> {
    // Start a local manager instance.
    let manager =
        Manager::new(zone, db.clone(), Arc::new(crypto::random::global_rng())).into_service();
    let manager_channel = Arc::new(rpc::LocalChannel::new(manager));
    let manager_stub = cluster_client::ManagerStub::new(manager_channel);

    // Start the CA
    println!("Starting CA job");
    let ca_job_spec = get_ca_job().await?;

    // TODO: Have a CLI progress bar for the uploading of the blob.
    start_job_impl(
        meta_client.clone(),
        &manager_stub,
        &ca_job_spec,
        &request_context,
    )
    .await?;

    // Since the node can't yet query any CA job, it can't create the CA by itself
    // yet. So we need to manually start the first CA worker with a locally
    // generated certificate.
    {
        let workers = db.list::<WorkerMetadataTable>().await?;
        if workers.len() != 1 {
            return Err(err_msg("Expected there to be exactly one worker."));
        }

        if workers[0].assigned_node() != node_id
            || !workers[0]
                .spec()
                .name()
                .starts_with("system.cert-authority.")
        {
            return Err(err_msg("Unexpected first CA worker spec state"));
        }

        // TODO: Try to dedup the logic for doing cert generation a big more.

        let cert = {
            let private_key =
                crypto::x509::PrivateKey::generate(crypto::x509::PrivateKeyType::ECDSA_SECP256R1)
                    .await?;

            let mut csr = crypto::x509::CertificateRequestBuilder::default();

            let name = ServiceName::for_worker(zone, workers[0].spec().name())?;
            let cname = name.to_string();
            csr.set_common_name(&cname);

            let csr = csr.build(&private_key).await?;

            let cert =
                sign_leaf_certificate(&name, csr, &root_creds.certificate, &root_creds.private_key)
                    .await?;

            let mut out = CertificateSecrets::default();
            out.set_private_key(private_key.to_der());
            out.add_certificates(cert.to_der().into());

            out
        };

        // Wait for the CA certificate to show up in the root registry
        // TODO

        // TODO: Document that if the cluster is offline for too long, it may need to be
        // re-bootstraped since worker certs will expire.

        let mut req = StartWorkerRequest::default();
        req.set_spec(workers[0].spec().clone());
        req.set_revision(workers[0].revision());
        req.set_credentials(cert);

        node.service
            .StartWorker(&request_context, &req)
            .await
            .result?;
    }

    /*
    TODO: Must wait for it to be healthy.

    Or minimally check for "state: RUNNING"
    */
    /*
    {
        for i in 0..10 {
            let res = node
                .service
                .ListWorkers(
                    &rpc::ClientRequestContext::default(),
                    &ListWorkersRequest::default(),
                )
                .await
                .result?;

            println!("{:?}", res);
            println!("====");

            executor::sleep(Duration::from_secs(1)).await?;
        }
    }
    */

    // First setup a local manager

    // Then we can attempt to start the CA
    // (and give it a bit of a helping hand)

    // Id of the local metastore server which is being used just for bootstrapping
    // the server.
    let local_server_id = {
        let status = meta_client.inner().current_status().await?;
        if status.configuration().servers_len() != 1 {
            return Err(err_msg("Expected exactly one metastore replica initially"));
        }

        // TODO: Find a better way of ensuring that this is definately the server that
        // is running in the local worker.
        let server = status.configuration().servers().iter().next().unwrap();
        if server.role() != Configuration_ServerRole::MEMBER {
            return Err(err_msg("First raft server is not a member"));
        }

        server.id()
    };

    // For this to work, we first need a certificate authority.
    // - So how do I boot

    // 'ca.system.job.local.cluster.internal' is what the node can call to get
    // certificates -

    // TODO: Verify that this actually has created the worker
    println!("Starting metastore job");
    let meta_job_spec = get_metastore_job(zone).await?;
    start_job_impl(
        meta_client.clone(),
        &manager_stub,
        &meta_job_spec,
        &request_context,
    )
    .await?;

    // Wait for the metastore to become part of the group.
    println!("Waiting for metastore replica to join group");
    loop {
        let status = meta_client.inner().current_status().await?;

        let mut done = false;
        for server in status.configuration().servers() {
            if server.id() == local_server_id {
                continue;
            }

            println!(
                "Found server {} with role {:?}",
                server.id().value(),
                server.role()
            );
            if server.role() == Configuration_ServerRole::MEMBER {
                done = true;
                break;
            }
        }

        if done {
            break;
        }

        executor::sleep(Duration::from_secs(4)).await;
    }
    println!("=> Done");

    println!("Removing local metastore replica");
    meta_client.inner().remove_server(local_server_id).await?;
    {
        local_metastore_resource
            .add_cancellation_token(Arc::new(AlreadyCancelledToken::default()))
            .await;
        local_metastore_resource.wait_for_termination().await?;
        drop(local_metastore_resource);
    }

    loop {
        // Wait for the local server to no longer be the leader.
        let status = match meta_client.inner().current_status().await {
            Ok(v) => v,
            Err(e) => {
                /*
                This may have one of two errors:
                - Failing because we tried directly connecting to the local metastore
                - Failing indirectly because we connected to the second replica and it piped our request to the remote server.

                TODO: Eventually we want to ensure that all these errors are eliminated through graceful leader transition.
                */
                // if let Some(status) = e.downcast_ref::<rpc::Status>() {
                //     // Requests may fail if trying to contact the currently stopping server.
                //     if status.code() == rpc::StatusCode::Unavailable {
                //         executor::sleep(Duration::from_secs(4)).await;
                //         continue;
                //     }
                // }

                eprintln!("- Failure connecting to metastore: {}", e);
                executor::sleep(Duration::from_secs(4)).await?;
                continue;
            }
        };
        if status.id() == local_server_id
            || status
                .configuration()
                .servers()
                .iter()
                .find(|s| s.id() == local_server_id)
                .is_some()
        {
            executor::sleep(Duration::from_secs(4)).await?;
            continue;
        } else {
            break;
        }
    }
    println!("=> Done");

    let mut manager_job_spec = get_manager_job().await?;

    start_job_impl(
        meta_client,
        &manager_stub,
        &manager_job_spec,
        &request_context,
    )
    .await?;

    // TODO: Wait for the new manager to become healthy.

    Ok(())
}
