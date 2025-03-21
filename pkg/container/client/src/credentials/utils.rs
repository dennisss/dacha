use common::errors::*;

use crate::service::address::ServiceName;

pub fn get_server_peer_identity(context: &rpc::ServerRequestContext) -> Result<ServiceName> {
    let conn = context
        .connection
        .as_ref()
        .ok_or_else(|| err_msg("Not running over HTTP"))?;
    let tls = conn
        .tls
        .as_ref()
        .ok_or_else(|| err_msg("Not running with TLS"))?;

    // TODO: return a rpc status.
    let client_cert = tls
        .certificate
        .as_ref()
        .ok_or_else(|| err_msg("mTLS is required"))?;

    let cn = client_cert
        .subject()
        .common_name()?
        .ok_or_else(|| err_msg("Client certificate has no common name"))?;

    // TODO: Convert to RPC error.
    Ok(ServiceName::parse(&cn)?)
}
