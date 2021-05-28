use common::bytes::Bytes;

use crate::tls::extensions::{NamedGroup, SignatureScheme};
use crate::tls::handshake::CipherSuite;



/// Configuration for how a TLS client will negotiate a handshake with the
/// remote. It puts constrains on which types of encryption algorithms we will,
/// what information we will validate about the server (certificate), and what
/// credentials (certificate) we can use to authenticate ourselves to the server.
pub struct ClientOptions {

    /// If not empty, then we will initially try to offer keys for these groups
    /// to the server to use for (EC)DHE key exchange. 
    ///
    /// NOTE: Must be a subset of 'supported_groups'
    pub initial_keys_shared: Vec<NamedGroup>,

    // TODO: Alternatively 

    /// DNS name of the remote server. e.g. "google.com"
    pub hostname: String,

    pub alpn_ids: Vec<Bytes>,

    // TODO: Have an option for whether or not we require/should do a verification
    // of the server's certificate.

    /// Supported algorithms that can be used for encrypting application traffic.
    pub supported_cipher_suites: Vec<CipherSuite>,

    /// Supported groups when using (EC)DHE to perform initial key exchange.
    pub supported_groups: Vec<NamedGroup>,
    
    /// Supported algorithms to use when verifying certificates.
    pub supported_signature_algorithms: Vec<SignatureScheme>,

    /// If true, we will allow trust self-signed server certificates which
    /// aren't in our root of trust registry.
    ///
    /// All other checks such as the certificate chain having valid signatures,
    /// the certificate being valid at the current point in time, etc. will
    /// apply. 
    pub trust_server_certificate: bool
}

impl ClientOptions {
    pub fn recommended() -> Self {
        ClientOptions {
            // TODO: Should almost always have a value.
            hostname: String::new(),

            alpn_ids: vec![],

            initial_keys_shared: vec![NamedGroup::x25519],

            supported_cipher_suites: vec![
                // MUST implement
                CipherSuite::TLS_AES_128_GCM_SHA256,
                // SHOULD implement
                CipherSuite::TLS_AES_256_GCM_SHA384,
                // SHOULD implement
                CipherSuite::TLS_CHACHA20_POLY1305_SHA256,
            ],
            supported_groups: vec![
                // SHOULD support
                NamedGroup::x25519,
                // MUST implement
                NamedGroup::secp256r1,
                // optional
                NamedGroup::secp384r1,
            ],
            supported_signature_algorithms: vec![
                // These three are the minimum required set to implement.
                SignatureScheme::ecdsa_secp256r1_sha256,
                SignatureScheme::rsa_pkcs1_sha256,
                SignatureScheme::rsa_pss_rsae_sha256,
            ],

            trust_server_certificate: false
        }
    }
}


struct ServerOptions {

}