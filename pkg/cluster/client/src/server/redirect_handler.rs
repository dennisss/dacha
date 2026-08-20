use std::collections::HashMap;

use std::sync::Arc;

use common::errors::*;
use common::hash::FastHasherBuilder;
use cluster_proto::cluster::*;
use executor_multitask::ServiceResource;
use http::ServerHandler;
use rpc_util::AddReflection;
use http_util::{internal_server_error, bad_request, not_found, forbidden};
use http::status_code::{MOVED_PERMANENTLY, FOUND};

use crate::credentials::get_http_server_peer_identity;
use crate::meta::client::ClusterMetaClient;
use crate::server::acl::*;
use crate::service::address::ServiceName;

pub(super) struct RedirectHandler {
    permanent: bool,
    new_path: String, 
}

impl RedirectHandler {
    pub fn new(new_path: &str, permanent: bool) -> Self {
        Self {
            new_path: new_path.to_string(),
            permanent,
        }
    }

    async fn handle_request_impl<'a>(
        &self,
        mut request: http::Request,
        context: http::ServerRequestContext<'a>,
    ) -> http::Response {
        http::ResponseBuilder::new()
            .status(if self.permanent { MOVED_PERMANENTLY } else { FOUND })
            .header("Location", self.new_path.as_str())
            .body(http::EmptyBody())
            .build()
            .unwrap()
    }
}

#[async_trait]
impl http::ServerHandler for RedirectHandler {
    async fn handle_request<'a>(
        &self,
        request: http::Request,
        context: http::ServerRequestContext<'a>,
    ) -> http::Response {
        self.handle_request_impl(request, context).await
    }
}
