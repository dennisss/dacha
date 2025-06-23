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
use crate::server::status_handler::*;
use crate::server::redirect_handler::*;
use crate::service::address::ServiceName;

const DEFAULT_SERVICE_ACL_PROTO: &'static str = r#"
    rules: [
        {
            path: "/favicon.ico"
            is_directory: false
            principals: ["authenticated"]
        },
        {
            path: "/assets"
            is_directory: true
            principals: ["authenticated"]
        },
        {
            path: "/rpc/grpc.reflection.v1alpha.ServerReflection/ServerReflectionInfo"
            is_directory: false
            principals: ["group:cluster-readers"]
        },
        {
            path: "/profilez"
            is_directory: false
            principals: ["group:cluster-owners"]
        },
        {
            path: "/server"
            is_directory: true
            principals: ["group:cluster-owners"]
        }
    ]
"#;

async fn not_found_handle_request(mut req: http::Request) -> http::Response {
    http_util::not_found()
}

// TODO: By default, if not running in the cluster (e.g. on a local developer's machine, disallow remote connections or enforce that only that user can access everything)

/// NOTE: This struct only exists during construction of the server.
pub struct ClusterServer {
    port: u16,
    tls_options: Option<crypto::tls::ServerOptionsContainer>,
    db: Option<Arc<ProtobufDB>>,
    acl: ServiceACLProto,
    router: PathRouter<Box<dyn http::ServerHandler>>,
    rpc_handler: rpc::Http2RequestHandler,
    zone: String,
}

impl ClusterServer {
    pub fn new(port: u16, acl: ServiceACLProto, client: Arc<ClusterMetaClient>) -> Result<Self> {
        let mut full_acl = ServiceACLProto::default();
        protobuf::text::parse_text_proto(DEFAULT_SERVICE_ACL_PROTO, &mut full_acl)?;
        full_acl.merge_from(&acl)?;

        let mut rpc_handler = rpc::Http2RequestHandler::new();
        rpc_handler.set_base_path("/rpc");

        Ok(Self {
            zone: client.zone().to_string(),
            port,
            tls_options: client.creds().as_ref().map(|c| c.server.clone()),
            db: Some(client.db().clone()),
            acl: full_acl,
            router: PathRouter::default(),
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
        self
            .router
            .add_route(path, is_directory, Box::new(handler))
    }

    pub fn start(mut self) -> Result<Arc<dyn ServiceResource>> {
        // No dependency on having assets to run a server.
        // TODO: Ideally make this more explicit optin rather than silently exposing this directory.
        if file::try_project_dir().is_ok() {
            self.add_request_handler("/assets", true, web::assets_handler())?;
        }
        self.add_request_handler("/favicon.ico", false, http::HttpFn(not_found_handle_request))?;
        
        self.router.add_route(
            PROFILEZ_PATH,
            false,
            Box::new(ProfilezRequestHandler::default()),
        )?;

        // TODO: Add health and readiness/liveness checks

        self.rpc_handler.add_reflection()?;

        // When there is no '/' handler, redirect to '/server/status'.
        if self.router.route_paths().find(|v| *v == "/").is_none() {
            self.add_request_handler("/", false, RedirectHandler::new("/server/status", false));

            let mut acl = self.acl.new_rules();
            acl.set_path("/");
            acl.add_principals("authenticated".into());
        }

        // TODO: If there is no '/' route, make it redirect to '/server/status'

        // Must do this before adding the '/rpc' route.
        let mut route_paths = self.all_route_paths();
        route_paths.push("/server/status".to_string());
        self.router.add_route("/server/status", false, Box::new(StatusHandler::new(&self.zone, route_paths)));

        self.router
            .add_route("/rpc", true, Box::new(self.rpc_handler))?;

        let mut options = http::ServerOptions::default();
        options.port = Some(self.port);
        options.tls = self.tls_options;

        let handler = HttpHandler {
            acl: ServiceACL::create(self.acl, &self.zone, self.db)?,
            router: self.router,
        };

        let mut server = http::Server::new(handler, options);

        Ok(Arc::new(server.start()))
    }

    fn all_route_paths(&self) -> Vec<String> {
        let mut paths = self.router.route_paths().map(|s| s.to_string()).collect::<Vec<_>>();

        for service in self.rpc_handler.services() {
            for method in service.method_names() {
                paths.push(format!("/rpc/{}/{}", service.service_name(), method));
            }
        }

        paths.sort();

        paths
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
