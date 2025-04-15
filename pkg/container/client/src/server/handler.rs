use std::collections::HashMap;

use std::sync::Arc;

use common::errors::*;
use common::hash::FastHasherBuilder;
use container_proto::cluster::*;
use executor_multitask::ServiceResource;
use http::ServerHandler;
use rpc_util::AddReflection;
use http_util::{internal_server_error, bad_request, not_found, forbidden};

use crate::credentials::get_http_server_peer_identity;
use crate::meta::client::ClusterMetaClient;
use crate::server::acl::*;
use crate::server::router::PathRouter;
use crate::service::address::ServiceName;

use super::ClusterServerHandlerData;

pub(super) struct HttpHandler {
    pub(super) acl: ServiceACL,
    pub(super) router: PathRouter<Box<dyn http::ServerHandler>>,
}

impl HttpHandler {
    async fn handle_connection_impl(&self, context: &mut http::ServerConnectionContext) -> bool {
        // TODO: Passthrough connection handling to the individual handlers?

        let peer = match get_http_server_peer_identity(context) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("[ServiceACL Reject] Invalid identity: {}", e);
                return false;
            }
        };

        if !self.acl.allow_unauthenticated() && peer.is_none() {
            eprintln!("[ServiceACL Reject] Unauthenticated Connection");
            return false;
        }

        context.handler_data = Some(Arc::new(ClusterServerHandlerData { peer }));

        true
    }

    async fn handle_request_impl<'a>(
        &self,
        mut request: http::Request,
        context: http::ServerRequestContext<'a>,
    ) -> http::Response {
        let cluster_context =
            match ClusterServerHandlerData::from_http_context(context.connection_context) {
                Ok(v) => v,
                Err(_) => return internal_server_error(),
            };

        // TODO: Add some warnings in the documentation that the raw HTTP server will
        // have the URis non-normalized.

        request.head.uri = match request.head.uri.normalized() {
            Ok(v) => v,
            Err(_) => return bad_request(),
        };

        if !request.head.uri.path.as_str().starts_with("/") {
            return bad_request();
        }

        let allowed = match self
            .acl
            .is_allowed(cluster_context.peer.as_ref(), &request)
            .await
        {
            Ok(v) => v,
            Err(e) => {
                eprintln!("ACL Check Failed: {}", e);
                return internal_server_error();
            }
        };

        // TODO: Probably worth converting to an RPC error if the client is using the
        // RPC path?
        if !allowed {
            eprintln!("[ServiceACL Reject] {}", request.head.uri.path.as_str());

            // TODO: Would be good to add a rejection metric.
            return forbidden();
        }

        let handler = match self.router.route(request.head.uri.path.as_str()) {
            Some((_, v)) => v,
            None => return not_found(),
        };

        handler.handle_request(request, context).await
    }
}

#[async_trait]
impl http::ServerHandler for HttpHandler {
    async fn handle_connection(&self, context: &mut http::ServerConnectionContext) -> bool {
        self.handle_connection_impl(context).await
    }

    async fn handle_request<'a>(
        &self,
        request: http::Request,
        context: http::ServerRequestContext<'a>,
    ) -> http::Response {
        self.handle_request_impl(request, context).await
    }
}
