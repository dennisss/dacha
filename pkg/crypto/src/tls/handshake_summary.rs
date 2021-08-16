use std::sync::Arc;

use common::bytes::Bytes;

use crate::x509;

/// Contains any interesting information collected during the TLS handshake.
#[derive(Default)]
pub struct HandshakeSummary {
    /// If ALPN ids were given by the client, this will be which one of them
    /// was selected by the server.
    ///
    /// When ALPN ids were given by the client, this will be None if and only
    /// if the server doesn't support ALPN extensions.
    pub selected_alpn_protocol: Option<Bytes>,

    /// Certificate if any which was received from the other endpoint. This
    /// certificate can be assumed to have already been validated for having
    /// a valid expiration time, chain of trust, and its private keys have
    /// been verified to be known by the remote endpoint.
    pub certificate: Option<Arc<x509::Certificate>>,
}
