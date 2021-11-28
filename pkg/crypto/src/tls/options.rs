use std::sync::Arc;

use common::bytes::Bytes;
use common::errors::*;

use crate::tls::cipher_suite::CipherSuite;
use crate::tls::extensions::{NamedGroup, SignatureScheme};
use crate::x509;

/// Configuration for how a TLS client will negotiate a handshake with the
/// remote. It puts constrains on which types of encryption algorithms we will,
/// what information we will validate about the server (certificate), and what
/// credentials (certificate) we can use to authenticate ourselves to the
/// server.
#[derive(Clone)]
pub struct ClientOptions {
    /// If not empty, then we will initially try to offer keys for these groups
    /// to the server to use for (EC)DHE key exchange.
    ///
    /// This mainly effects TLS 1.3 connections.
    ///
    /// NOTE: Must be a subset of 'supported_groups'
    pub initial_keys_shared: Vec<NamedGroup>,

    /// DNS name of the remote server. e.g. "google.com"
    pub hostname: String,

    pub alpn_ids: Vec<Bytes>,

    // TODO: Have an option for whether or not we require/should do a verification
    // of the server's certificate.
    /// Supported algorithms that can be used for encrypting application
    /// traffic.
    pub supported_cipher_suites: Vec<CipherSuite>,

    /// Supported groups when using (EC)DHE to perform initial key exchange.
    pub supported_groups: Vec<NamedGroup>,

    /// Supported algorithms to use when verifying certificates.
    pub supported_signature_algorithms: Vec<SignatureScheme>,

    pub certificate_request: CertificateRequestOptions,

    /// If present, the client will support authenticating using a certificate
    /// in response to a server CertificateRequest.
    pub certificate_auth: Option<CertificateAuthenticationOptions>,
}

impl ClientOptions {
    /// Also using this, you probably want to set the 'hostname' field.
    pub fn recommended() -> Self {
        Self {
            // TODO: Should almost always have a value.
            hostname: String::new(),

            alpn_ids: vec![],

            initial_keys_shared: vec![NamedGroup::x25519],

            supported_cipher_suites: vec![
                // SHOULD implement
                CipherSuite::TLS_CHACHA20_POLY1305_SHA256,
                // MUST implement
                CipherSuite::TLS_AES_128_GCM_SHA256,
                // SHOULD implement
                CipherSuite::TLS_AES_256_GCM_SHA384,
                // TLS 1.2 Only
                CipherSuite::TLS_ECDHE_ECDSA_WITH_CHACHA20_POLY1305_SHA256,
                CipherSuite::TLS_ECDHE_RSA_WITH_CHACHA20_POLY1305_SHA256,
                CipherSuite::TLS_ECDHE_ECDSA_WITH_AES_128_GCM_SHA256,
                CipherSuite::TLS_ECDHE_RSA_WITH_AES_128_GCM_SHA256,
                /*
                 * CipherSuite::TLS_ECDHE_ECDSA_WITH_AES_256_GCM_SHA384,
                 * CipherSuite::TLS_ECDHE_ECDSA_WITH_AES_128_CBC_SHA256,
                 * CipherSuite::TLS_ECDHE_ECDSA_WITH_AES_256_CBC_SHA384,
                 *
                 * CipherSuite::TLS_ECDHE_RSA_WITH_AES_256_GCM_SHA384,
                 * CipherSuite::TLS_ECDHE_RSA_WITH_AES_128_CBC_SHA256,
                 * CipherSuite::TLS_ECDHE_RSA_WITH_AES_256_CBC_SHA384, */
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
                // TLS 1.3: These three are the minimum required set to implement.
                SignatureScheme::ecdsa_secp256r1_sha256,
                SignatureScheme::rsa_pss_rsae_sha256,
                SignatureScheme::rsa_pkcs1_sha256,
                // Extra to allow old TLS 1.2 servers to have a decent fallback.
                SignatureScheme::rsa_pkcs1_sha384,
                SignatureScheme::rsa_pkcs1_sha512,
            ],

            certificate_request: CertificateRequestOptions {
                root_certificate_registry: CertificateRegistrySource::PublicRoots,
                trust_remote_certificate: false,
            },

            certificate_auth: None,
        }
    }
}

#[derive(Clone)]
pub struct ServerOptions {
    /// If present, will be used to make a CertificateRequest to the client.
    ///
    /// NOTE: We do not
    pub certificate_request: Option<CertificateRequestOptions>,

    pub certificate_auth: CertificateAuthenticationOptions,

    /// Protocol ids to accept in the order from highest to lowest preferance.
    /// TODO: Support rejecting requests that don't have a negotiated protocol?
    pub alpn_ids: Vec<Bytes>,

    pub supported_cipher_suites: Vec<CipherSuite>,

    pub supported_groups: Vec<NamedGroup>,

    /// Should also be used for validating client certificates.
    pub supported_signature_algorithms: Vec<SignatureScheme>,
}

impl ServerOptions {
    pub fn recommended(certificate_file: Bytes, private_key_file: Bytes) -> Result<Self> {
        let client_options = ClientOptions::recommended();

        Ok(Self {
            certificate_request: None,
            certificate_auth: CertificateAuthenticationOptions::create(
                certificate_file,
                private_key_file,
            )?,
            alpn_ids: vec![],
            supported_cipher_suites: client_options.supported_cipher_suites,
            supported_groups: client_options.supported_groups,
            supported_signature_algorithms: client_options.supported_signature_algorithms,
        })
    }
}

#[derive(Clone)]
pub struct CertificateRequestOptions {
    /// Registry to use for validating server certificates.
    /// If using trust_server_certificate==true, this can be empty.
    ///
    /// This won't be used directly but rather the registry that results from
    /// combining this with any additional certificates provided by the remote
    /// endpoint will be used.
    ///
    /// Self::recommended() will default to using a set of public CA's for this.
    ///
    /// NOTE: Currently we assume that the CA certificates don't need to be
    /// remoted/invokes while this client is in use.
    pub root_certificate_registry: CertificateRegistrySource,

    /// If true, we will allow trust self-signed server certificates which
    /// aren't in our root of trust registry.
    ///
    /// All other checks such as the certificate chain having valid signatures,
    /// the certificate being valid at the current point in time, etc. will
    /// apply.
    pub trust_remote_certificate: bool,
}

#[derive(Clone)]
pub struct CertificateAuthenticationOptions {
    /// Certificates to advertise to the client.
    ///
    /// This must contain at least 1 certificate where:
    /// - the first certificate corresponds to this server's identity
    ///   - We'll reject any client request that requests a host name not named
    ///     in this certificate
    ///   - We will verify to the client that we own this certificate using the
    ///     below private key.
    /// - other certificates are simply passed along to the client without extra
    ///   processing.
    ///   - This is mainly to provide the client with the whole chain of trust
    ///     if we believe the client doesn't know it.
    ///
    /// NOTE: We currently only support using a server having a single identity
    /// certificate (so if the server will be used to server as multiple host
    /// names they must all be in the same certificate),
    pub certificates: Vec<Arc<x509::Certificate>>,

    pub private_key: x509::PrivateKey,
}

impl CertificateAuthenticationOptions {
    pub fn create(certificate_file: Bytes, private_key_file: Bytes) -> Result<Self> {
        let certificates = x509::Certificate::from_pem(certificate_file)?;
        let private_key = x509::PrivateKey::from_pem(private_key_file)?;
        Ok(Self {
            certificates,
            private_key,
        })
    }
}

#[derive(Clone)]
pub enum CertificateRegistrySource {
    PublicRoots,
    Custom(Arc<x509::CertificateRegistry>),
}

impl CertificateRegistrySource {
    pub async fn resolve(&self) -> Result<Arc<x509::CertificateRegistry>> {
        Ok(match self {
            CertificateRegistrySource::PublicRoots => {
                Arc::new(x509::CertificateRegistry::public_roots().await?)
            }
            CertificateRegistrySource::Custom(v) => v.clone(),
        })
    }
}
