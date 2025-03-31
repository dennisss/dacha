use std::sync::Arc;

use cluster_client::id::entity_id_from_string;
use cluster_client::meta::hostname::ClusterMetaHostnameResolver;
use cluster_client::meta::KeyPrefixACLTable;
use cluster_client::ClusterServer;
use common::args::list::CommaSeparated;
use common::args::parse_args;
use common::errors::*;
use datastore::meta::store::MetastoreOptions;
use datastore::meta::EmbeddedDBStateMachineOptions;
use executor_multitask::{RootResource, ServiceResource, ServiceResourceGroup};
use file::LocalPathBuf;
use raft::{log::segmented_log::SegmentedLogOptions, proto::RouteLabel};
use rpc_util::NamedPortArg;

use crate::acl::KeyPrefixACLProcessor;

const SERVICE_ACL_PROTO: &'static str = r#"

    allow_unauthenticated: false

    rules: [
        # Risky RPCs. Can only be used between metastore instances.
        {
            path: "/rpc/raft.Consensus"
            is_directory: true
            # TODO: The '<zone>' here is annoying.
            principals: ["dns:meta.system.job.<zone>.cluster.internal"]
        },
        {
            path: "/rpc/db.meta.ServerManagement"
            is_directory: true,
            principals: ["dns:meta.system.job.<zone>.cluster.internal"]
        },
        {
            path: "/rpc/raft.Discovery/Announce"
            is_directory: false,
            principals: ["dns:meta.system.job.<zone>.cluster.internal"]
        },

        # RPCs do their own ACL validation per key range.
        {
            path: "/rpc/db.meta.KeyValueStore"
            is_directory: true
            principals: ["authenticated"]
        },
        {
            path: "/rpc/db.meta.ClientManagement"
            is_directory: true
            principals: ["authenticated"]
        },

        # Read-only
        {
            path: "/rpc/raft.Discovery/Read"
            is_directory: false,
            principals: ["authenticated"]
        }

    ]
"#;

pub struct ClusterMetastoreOptions {
    pub id: u64,
    pub port: u16,
    pub zone: String,
    pub creds: crypto::tls::Credentials,
    pub dir: LocalPathBuf,
    pub bootstrap: bool,
}

pub async fn run(options: ClusterMetastoreOptions) -> Result<Arc<dyn ServiceResource>> {
    let mut resources = Arc::new(ServiceResourceGroup::new("Metastore"));

    let acl_processor = Arc::new(KeyPrefixACLProcessor::new(&options.zone));

    let mut route_label = RouteLabel::default();
    route_label.set_value(format!(
        "{}={}",
        cluster_client::env::ZONE_ENV_VAR,
        &options.zone
    ));

    let mut state_machine = EmbeddedDBStateMachineOptions::default();
    state_machine.processor = Some(acl_processor.clone());

    let mut acl = container_proto::cluster::ServiceACLProto::default();
    protobuf::text::parse_text_proto(
        &SERVICE_ACL_PROTO.replace("<zone>", &options.zone),
        &mut acl,
    )?;

    // TODO: Must limit what percentage of request slots can be used for user facing
    // requests since we also use this for server-to-server Raft requests.
    let mut server = ClusterServer::new_internal(
        options.port,
        acl,
        &options.zone,
        None,
        Some(options.creds.server.clone()),
    )?;

    resources
        .register_dependency(
            datastore::meta::store::run(
                MetastoreOptions {
                    dir: options.dir,
                    init_port: None,
                    bootstrap_group: options.bootstrap,
                    bootstrap_node_id: Some(options.id),
                    service_port: options.port,
                    route_labels: vec![route_label],
                    log: SegmentedLogOptions::default(),
                    state_machine,
                    tls: Some(options.creds.clone()),
                    hostname_resolver: Arc::new(ClusterMetaHostnameResolver::new(&options.zone)),
                    acl_processor: Some(acl_processor.clone()),
                },
                &mut server,
            )
            .await?,
        )
        .await;

    resources.register_dependency(server.start()?).await;

    Ok(resources)
}
