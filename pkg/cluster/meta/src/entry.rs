use std::sync::Arc;

use cluster_client::id::entity_id_from_string;
use cluster_client::meta::hostname::ClusterMetaHostnameResolver;
use cluster_client::meta::KeyPrefixACLTable;
use cluster_client::{ClusterServer, ClusterMetaClient};
use common::args::list::CommaSeparated;
use common::args::parse_args;
use common::errors::*;
use db_txn::{EmbeddedDBStateMachineOptions, TransactionalDB, TransactionalDBOptions};
use executor::sync::Eventually;
use executor_multitask::{RootResource, ServiceResource, ServiceResourceGroup};
use file::LocalPathBuf;
use raft::{log::segmented_log::SegmentedLogOptions, proto::RouteLabel};
use rpc_util::NamedPortArg;
use container_proto::cluster::ServiceResolverIntoService;

use crate::acl::KeyPrefixACLProcessor;
use crate::resolver::ServiceResolverImpl;

const SERVICE_ACL_PROTO: &'static str = r#"

    allow_unauthenticated: true

    rules: [
        # Risky RPCs. Can only be used between metastore instances.
        {
            path: "/rpc/raft.Consensus"
            is_directory: true
            principals: ["dns:meta.system.job.local.cluster.internal"]
        },
        {
            path: "/rpc/db.meta.ServerManagement"
            is_directory: true,
            principals: ["dns:meta.system.job.local.cluster.internal"]
        },
        {
            path: "/rpc/raft.Discovery/Announce"
            is_directory: false,
            principals: ["dns:meta.system.job.local.cluster.internal"]
        },

        # RPCs do their own ACL validation per key range.
        {
            path: "/rpc/db.txn.KeyValueStore"
            is_directory: true
            principals: ["authenticated"]
        },
        {
            path: "/rpc/db.txn.ClientManagement"
            is_directory: true
            principals: ["authenticated"]
        },

        # Read-only
        {
            path: "/rpc/raft.Discovery/Read"
            is_directory: false,
            principals: ["authenticated"]
        },
        # Does its own ACL checks.
        # Must be accessible by arbitrary clients to resolve services needed for login.
        {
            path: "/rpc/cluster.ServiceResolver/Resolve"
            is_directory: false
            principals: ["unauthenticated"]
        }
    ]
"#;

pub struct ClusterMetastoreOptions {
    pub id: u64,
    pub port: u16,
    
    /// NOTE: This should be tracked as a resource by the caller of run().
    pub client: Arc<ClusterMetaClient>,
    
    pub dir: LocalPathBuf,
    pub bootstrap: bool,
}

pub async fn run(options: ClusterMetastoreOptions) -> Result<Arc<dyn ServiceResource>> {
    let mut resources = Arc::new(ServiceResourceGroup::new("Metastore"));

    let acl_processor = Arc::new(KeyPrefixACLProcessor::new(options.client.zone()));

    let mut route_label = RouteLabel::default();
    route_label.set_value(format!(
        "{}={}",
        cluster_client::env::ZONE_ENV_VAR,
        options.client.zone()
    ));

    let mut state_machine = EmbeddedDBStateMachineOptions::default();
    state_machine.processor = Some(acl_processor.clone());

    let mut acl = container_proto::cluster::ServiceACLProto::default();
    protobuf::text::parse_text_proto(SERVICE_ACL_PROTO, &mut acl)?;

    // TODO: Must limit what percentage of request slots can be used for user facing
    // requests since we also use this for server-to-server Raft requests.
    let mut server = ClusterServer::new(
        options.port,
        acl,
        options.client.clone()
    )?;

    let rpc_server_ready = Arc::new(Eventually::new());

    resources
        .register_dependency(
            TransactionalDB::create(
                TransactionalDBOptions {
                    dir: options.dir,
                    bootstrap_group: options.bootstrap,
                    bootstrap_node_id: Some(options.id),
                    service_port: options.port,
                    route_labels: vec![route_label],
                    log: SegmentedLogOptions::default(),
                    state_machine,
                    tls: options.client.creds(),
                    hostname_resolver: Arc::new(ClusterMetaHostnameResolver::new(options.client.zone())),
                    acl_processor: Some(acl_processor.clone()),
                },
                &mut server,
                rpc_server_ready.clone(),
            )
            .await?,
        )
        .await;

    server.add_service(ServiceResolverImpl::new(
        options.client.db().clone(), options.client.zone()).into_service()
    );

    let rpc_server = server.start()?;
    resources.register_dependency(rpc_server.clone()).await;

    executor::spawn(async move {
        rpc_server.wait_for_ready().await;
        let _ = rpc_server_ready.set(()).await;
    });

    Ok(resources)
}
