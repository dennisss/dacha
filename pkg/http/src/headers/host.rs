use common::errors::*;

use crate::header::{Headers, HOST};
use crate::uri::Authority;
use crate::uri_syntax::parse_authority;

/// NOTE: The host header will never contain the 'userinfo' component. Only a
/// 'host' and 'port'.
pub fn parse_host_header(headers: &Headers) -> Result<Option<Authority>> {
    let mut iter = headers.find(HOST);

    let header = match iter.next() {
        Some(header) => header,
        None => {
            return Ok(None);
        }
    };

    if iter.next().is_some() {
        return Err(err_msg("More than one \"Host\" header"));
    }

    let (authority, _) = parsing::complete(parse_authority)(header.value.to_bytes())?;
    if authority.user.is_some() {
        return Err(err_msg("Host header should not contain userinfo"));
    }

    Ok(Some(authority))
}
