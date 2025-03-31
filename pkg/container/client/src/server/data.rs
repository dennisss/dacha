use std::sync::Arc;

use common::errors::*;

use crate::service::address::ServiceName;

pub struct ClusterServerHandlerData {
    pub peer: Option<ServiceName>,
}

impl ClusterServerHandlerData {
    pub fn from_rpc_context(
        context: &rpc::ServerRequestContext,
    ) -> Result<Arc<ClusterServerHandlerData>> {
        let conn = context
            .connection
            .as_ref()
            .ok_or_else(|| err_msg("Not running over HTTP"))?;

        Self::from_http_context(conn)
    }

    pub fn from_http_context(
        context: &http::ServerConnectionContext,
    ) -> Result<Arc<ClusterServerHandlerData>> {
        let data = context
            .handler_data
            .clone()
            .ok_or_else(|| err_msg("Missing handler_data"))?;

        data.downcast()
            .map_err(|_| err_msg("Not running in a ClusterServer"))
    }
}
