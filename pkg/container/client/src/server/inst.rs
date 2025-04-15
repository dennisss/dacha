use std::collections::HashMap;

use core::ops::{Deref, DerefMut};
use std::sync::Arc;

use common::errors::*;
use container_proto::cluster::*;
use db_table::db::ProtobufDB;
use executor_multitask::ServiceResource;
use http::ServerHandler;
use rpc_util::AddReflection;
use rpc_util::ProfilezRequestHandler;
use rpc_util::PROFILEZ_PATH;
use protobuf::Message;

use crate::acl::checker::*;
use crate::credentials::get_http_server_peer_identity;
use crate::meta::client::ClusterMetaClient;
use crate::server::acl::*;
use crate::server::handler::*;
use crate::server::router::PathRouter;
use crate::service::address::ServiceName;

const DEFAULT_SERVICE_ACL_PROTO: &'static str = r#"
    rules: [
        {
            path: "/rpc/grpc.reflection.v1alpha.ServerReflection/ServerReflectionInfo"
            is_directory: false
            principals: ["group:cluster-readers"]
        },
        {
            path: "/profilez"
            is_directory: false
            principals: ["group:cluster-admins"]
        }
    ]
"#;

// TODO: By default, if not running in the cluster (e.g. on a local developer's machine, disallow remote connections or enforce that only that user can access everything)

/// NOTE: This struct only exists during construction of the server.
pub struct ClusterServer {
    port: u16,
    tls_options: Option<crypto::tls::ServerOptionsContainer>,
    handler: HttpHandler,
    rpc_handler: rpc::Http2RequestHandler,
}

impl ClusterServer {
    pub fn new(port: u16, acl: ServiceACLProto, client: Arc<ClusterMetaClient>) -> Result<Self> {
        let mut full_acl = ServiceACLProto::default();
        protobuf::text::parse_text_proto(DEFAULT_SERVICE_ACL_PROTO, &mut full_acl)?;
        full_acl.merge_from(&acl)?;

        let mut inst = Self::new_internal(
            port,
            full_acl,
            client.zone(),
            Some(client.db().clone()),
            client.creds().as_ref().map(|c| c.server.clone()),
        )?;

        Ok(inst)
    }

    /// NOTE: This constructor is just used in environments where we can't
    /// depend on the metastore (e.g. in the metastore).
    pub fn new_internal(
        port: u16,
        acl: ServiceACLProto,
        zone: &str,
        db: Option<Arc<ProtobufDB>>,
        tls_options: Option<crypto::tls::ServerOptionsContainer>,
    ) -> Result<Self> {
        let mut rpc_handler = rpc::Http2RequestHandler::new();
        rpc_handler.set_base_path("/rpc");

        Ok(Self {
            port,
            tls_options,
            handler: HttpHandler {
                acl: ServiceACL::create(acl, zone, db)?,
                router: PathRouter::default(),
            },
            rpc_handler,
        })
    }

    ///
    ///
    ///
    /// WARNING: None of the handle_connection methods on the ServerHandlers
    /// will be called.
    pub fn add_request_handler<H: http::ServerHandler>(
        &mut self,
        path: &str,
        is_directory: bool,
        handler: H,
    ) -> Result<()> {
        self.handler
            .router
            .add_route(path, is_directory, Box::new(handler))
    }

    pub fn start(mut self) -> Result<Arc<dyn ServiceResource>> {
        self.handler.router.add_route(
            PROFILEZ_PATH,
            false,
            Box::new(ProfilezRequestHandler::default()),
        )?;

        // TODO: Add health and readiness/liveness checks

        self.rpc_handler.add_reflection()?;

        // self.debug_print();

        self.handler
            .router
            .add_route("/rpc", true, Box::new(self.rpc_handler))?;

        let mut options = http::ServerOptions::default();
        options.port = Some(self.port);
        options.tls = self.tls_options;

        let mut server = http::Server::new(self.handler, options);

        Ok(Arc::new(server.start()))
    }

    fn debug_print(&self) {
        let mut paths = self.handler.router.route_paths().collect::<Vec<_>>();
        paths.push("/rpc");
        paths.sort();

        for path in paths {
            if path == "/rpc" {
                for service in self.rpc_handler.services() {
                    for method in service.method_names() {
                        println!("[Route] /rpc/{}/{}", service.service_name(), method);
                    }
                }
            } else {
                println!("[Route] {}", path);
            }
        }
    }
}

impl Deref for ClusterServer {
    type Target = rpc::Http2RequestHandler;

    fn deref(&self) -> &rpc::Http2RequestHandler {
        &self.rpc_handler
    }
}

impl DerefMut for ClusterServer {
    fn deref_mut(&mut self) -> &mut rpc::Http2RequestHandler {
        &mut self.rpc_handler
    }
}
