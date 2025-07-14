use std::sync::Arc;

use common::errors::*;

use crate::service::address::ServiceName;

pub struct ClusterServerConnectionData {
    /// Identity of the entity directly calling this server.
    pub peer: Option<ServiceName>,
}

impl ClusterServerConnectionData {
    pub fn from_rpc_context(
        context: &rpc::ServerRequestContext,
    ) -> Result<Arc<Self>> {
        let conn = context
            .connection
            .as_ref()
            .ok_or_else(|| err_msg("Not running over HTTP"))?;

        Self::from_http_context(conn)
    }

    pub fn from_http_context(
        context: &http::ServerConnectionContext,
    ) -> Result<Arc<Self>> {
        let data = context
            .handler_data
            .clone()
            .ok_or_else(|| err_msg("Missing handler_data"))?;

        data.downcast()
            .map_err(|_| err_msg("Not running in a ClusterServer"))
    }
}

pub struct ClusterServerRequestData {
    /// Identity to be used for ACL checks. Defaults to the peer user of the request.
    pub effective_entity: Option<ServiceName>,
}

impl ClusterServerRequestData {
    pub fn from_rpc_context(
        context: &rpc::ServerRequestContext,
    ) -> Result<Arc<Self>> {
        let data = context
            .handler_data
            .clone()
            .ok_or_else(|| err_msg("Not running over HTTP"))?;

        data.downcast()
            .map_err(|_| err_msg("Not running in a ClusterServer"))
    }

    pub fn from_http_context(
        context: &http::ServerRequestContext,
    ) -> Result<Arc<Self>> {
        let data = context
            .handler_data
            .clone()
            .ok_or_else(|| err_msg("Missing handler_data"))?;

        data.downcast()
            .map_err(|_| err_msg("Not running in a ClusterServer"))
    }
}