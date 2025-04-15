use common::errors::*;

use crate::service::address::ServiceName;

pub(crate) fn get_http_server_peer_identity(
    conn: &http::ServerConnectionContext,
) -> Result<Option<ServiceName>> {
    let tls = conn
        .tls
        .as_ref()
        .ok_or_else(|| err_msg("Not running with TLS"))?;

    // TODO: return a rpc status.
    let client_cert = match tls.certificate.as_ref() {
        Some(v) => v,
        None => return Ok(None),
    };

    let cn = client_cert
        .subject()
        .common_name()?
        .ok_or_else(|| err_msg("Client certificate has no common name"))?;

    // TODO: Convert to RPC error.
    Ok(Some(ServiceName::parse(&cn)?))
}

pub(crate) fn get_server_peer_identity(context: &rpc::ServerRequestContext) -> Result<ServiceName> {
    let conn = context
        .connection
        .as_ref()
        .ok_or_else(|| err_msg("Not running over HTTP"))?;

    let ident = get_http_server_peer_identity(conn)?.ok_or_else(|| err_msg("mTLS is required"))?;

    Ok(ident)
}
