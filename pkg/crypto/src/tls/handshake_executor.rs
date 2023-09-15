use std::convert::TryInto;
use std::sync::Arc;

use asn::encoding::DERWriteable;
use common::bytes::Bytes;
use common::errors::*;

use crate::elliptic::EllipticCurveGroup;
use crate::hasher::{GetHasherFactory, HasherFactory};
use crate::tls::alert::AlertLevel;
use crate::tls::constants::*;
use crate::tls::extensions::SignatureScheme;
use crate::tls::handshake::{Certificate, CertificateEntry, CertificateVerify, Handshake};
use crate::tls::key_schedule::KeySchedule;
use crate::tls::record_stream::{Message, RecordReader, RecordWriter};
use crate::tls::transcript::Transcript;
use crate::x509;

use super::{CertificateAuthenticationOptions, CertificateRequestOptions};

const TLS13_CERTIFICATEVERIFY_CLIENT_CTX: &'static [u8] = b"TLS 1.3, client CertificateVerify";
const TLS13_CERTIFICATEVERIFY_SERVER_CTX: &'static [u8] = b"TLS 1.3, server CertificateVerify";

pub(super) struct HandshakeExecutorOptions<'a> {
    pub is_server: bool,

    pub local_supported_algorithms: &'a [SignatureScheme],
    // pub certificate_request: Option<&'a >
}

// TODO: Limit the max size of handshake state on the server (especially if we
// get huge certificates to parse) (or just allow disabling certificate
// authentication).

/// Common interface for executing client/server TLS handshakes.
///
/// NOTE: This is an internal interface to be primarily used by the 'client' and
/// 'server' modules.
pub(super) struct HandshakeExecutor<'a> {
    pub reader: RecordReader,
    pub writer: RecordWriter,
    pub options: HandshakeExecutorOptions<'a>,
    pub handshake_transcript: Transcript,
}

impl<'a> HandshakeExecutor<'a> {
    pub fn new(
        reader: RecordReader,
        writer: RecordWriter,
        options: HandshakeExecutorOptions<'a>,
    ) -> Self {
        Self {
            reader,
            writer,
            options,
            handshake_transcript: Transcript::new(),
        }
    }

    pub async fn send_handshake_message(&mut self, handshake: Handshake) -> Result<()> {
        self.writer
            .send_handshake(handshake, Some(&mut self.handshake_transcript))
            .await
    }

    pub async fn receive_handshake_message(&mut self) -> Result<Handshake> {
        loop {
            let msg = self
                .reader
                .recv(Some(&mut self.handshake_transcript))
                .await?;

            match msg {
                Message::Handshake(m) => {
                    return Ok(m);
                }
                Message::ApplicationData(_) => {
                    return Err(err_msg("Unexpected application data during handshake"));
                }
                Message::ChangeCipherSpec(_) => {
                    // TODO: Improve this.
                    continue;
                }
                Message::Alert(alert) => {
                    if alert.level == AlertLevel::fatal {
                        println!("{:?}", alert);
                        return Err(err_msg("Received fatal alert"));
                    }

                    println!("Received Alert!");
                    continue;
                }
            };
        }
    }

    pub async fn send_certificate(
        &mut self,
        options: &CertificateAuthenticationOptions,
        certificate_request_context: Bytes,
    ) -> Result<()> {
        // TODO: In the future this will need to be clever enough to pick between
        // multiple certificates based on the supported signature algorithms on the
        // remote machine.

        // While it is technically allowed for a client to send 0 certificates (and skip
        // the CertificateVerify), we don't currently support this. For now the client
        // must just not set the certificate_auth options upfront if certificate
        // authentication shouldn't be performed.
        if options.certificates.len() < 1 {
            return Err(err_msg("Expected to send at least one certificate"));
        }

        // TODO: Must verify that this contains at least one certificate.
        let mut certificate_list = vec![];
        for cert in &options.certificates {
            certificate_list.push(CertificateEntry {
                cert: cert.raw.to_der().into(), /* TODO: Can we implement this without
                                                 * re-serialization. */
                extensions: vec![],
            });
        }

        let certs = Handshake::Certificate(Certificate {
            certificate_request_context,
            certificate_list,
        });

        self.send_handshake_message(certs).await
    }

    /// Receives the client/server's Certificate and verifies that:
    /// 1. The certificate is valid now
    /// 2. The certificate is valid for the remote host name.
    /// 3. The certificate forms a valid chain to a trusted root certificate.
    ///
    /// This is both TLS 1.2 and 1.3.
    pub async fn process_certificate(
        &mut self,
        cert: Certificate,
        certificate_registry: &mut x509::CertificateRegistry,
        options: &CertificateRequestOptions,
        remote_host_name: Option<&str>,
    ) -> Result<Option<Arc<x509::Certificate>>> {
        // On a client, the certicate received from the server should always have an
        // empty context and our server implementation doesn't use this field either, so
        // this should always be empty.
        if cert.certificate_request_context.len() != 0 {
            return Err(err_msg("Unexpected request context width certificate"));
        }

        let mut cert_list = vec![];
        for c in &cert.certificate_list {
            cert_list.push(Arc::new(x509::Certificate::read(c.cert.clone())?));
        }

        if cert_list.len() < 1 {
            return Ok(None);
        }

        // NOTE: This will return an error if any of the certificates are invalid.
        // TODO: Technically we only need to ensure that the first one is valid.
        certificate_registry.append(&cert_list, options.trust_remote_certificate)?;

        // Verify the terminal certificate is valid (MUST always be first).

        // TODO: How do we verify that all parent certificates are allowed to issue
        // sub-certificates.
        // - Must validate 'Certificate Basic Constraints' and 'Certificate Key Usage'
        //   to verify that certificates can be signed.

        // TODO: Have a max age to connections so that we eventually require re-checking
        // TLS certificate validity.

        if let Some(usage) = cert_list[0].key_usage()? {
            if !usage.digitalSignature().unwrap_or(false) {
                return Err(err_msg(
                    "Certificate can't be used for signature verification",
                ));
            }
        }

        // TODO: Remove the trust_remote_certificate exception and instead always check
        // this.
        if !options.trust_remote_certificate {
            if !cert_list[0].valid_now() {
                return Err(err_msg("Certificate not valid now"));
            }

            if let Some(name) = remote_host_name {
                if !cert_list[0].for_dns_name(name)? {
                    return Err(err_msg("Certificate not valid for DNS name"));
                }
            }
        }

        Ok(Some(cert_list.remove(0)))
    }

    /// Creates a TLS 1.3 CertificateVerify message to be sent after a
    /// Certificate.
    pub async fn create_certificate_verify(
        &self,
        key_schedule: &KeySchedule,
        remote_supported_algorithms: &[SignatureScheme],
        private_key: &x509::PrivateKey,
    ) -> Result<CertificateVerify> {
        // Transcript hash for ClientHello through to the Certificate.
        let ch_ct_transcript_hash = self
            .handshake_transcript
            .hash(key_schedule.hasher_factory());

        let mut plaintext = vec![];
        for _ in 0..64 {
            plaintext.push(0x20);
        }
        plaintext.extend_from_slice(if self.options.is_server {
            &TLS13_CERTIFICATEVERIFY_SERVER_CTX
        } else {
            &TLS13_CERTIFICATEVERIFY_CLIENT_CTX
        });
        plaintext.push(0);
        plaintext.extend_from_slice(&ch_ct_transcript_hash);

        // Select the best signature scheme.
        // - Must be supported by the client.
        // - Must be supported by our certificate/private key.
        let mut selected_signature_algorithm = None;
        for algorithm in self.options.local_supported_algorithms {
            if !remote_supported_algorithms.contains(algorithm) {
                continue;
            }

            let (signature_algorithm, constraints) = match algorithm.to_x509_signature_id() {
                Some(v) => v,
                None => continue,
            };

            if !private_key.can_create_signature(&signature_algorithm, &constraints)? {
                continue;
            }

            selected_signature_algorithm =
                Some((algorithm.clone(), signature_algorithm, constraints));
            break;
        }

        let (selected_signature_algorithm, signature_algorithm, constraints) =
            selected_signature_algorithm
                .ok_or_else(|| err_msg("Failed to get a good algorithm"))?;

        // TODO: Verify that rsa_pkcs1_sha256 is never used in TLS 1.3

        let signature = private_key
            .create_signature(&plaintext, &signature_algorithm, &constraints)
            .await?;

        Ok(CertificateVerify {
            algorithm: selected_signature_algorithm,
            signature: (*signature).as_ref().to_vec().into(),
        })
    }

    /// Receives a TLS 1.3 CertificateVerify message from a remote client or
    /// server and verifies that it is valid.
    pub async fn receive_certificate_verify_v13(
        &mut self,
        cert: &x509::Certificate,
        hasher_factory: &HasherFactory,
        certificate_registry: &x509::CertificateRegistry,
    ) -> Result<()> {
        // Transcript hash for ClientHello through to the Certificate.
        let ch_ct_transcript_hash = self.handshake_transcript.hash(&hasher_factory);

        let cert_verify = match self.receive_handshake_message().await? {
            Handshake::CertificateVerify(c) => c,
            _ => {
                return Err(err_msg("Expected certificate verify"));
            }
        };

        let mut plaintext = vec![];
        for _ in 0..64 {
            plaintext.push(0x20);
        }
        plaintext.extend_from_slice(if self.options.is_server {
            &TLS13_CERTIFICATEVERIFY_CLIENT_CTX
        } else {
            &TLS13_CERTIFICATEVERIFY_SERVER_CTX
        });
        plaintext.push(0);
        plaintext.extend_from_slice(&ch_ct_transcript_hash);

        if self
            .options
            .local_supported_algorithms
            .iter()
            .find(|a| **a == cert_verify.algorithm)
            .is_none()
        {
            // TODO: This may happen if no certificate exists that can be used with any of
            // the requested algorithms.
            return Err(err_msg(
                "Received certificate verification with non-advertised algorithm.",
            ));
        }

        /*
        TODO:
        For TLS 1.3:
        RSA signatures MUST use an RSASSA-PSS algorithm, regardless of whether RSASSA-PKCS1-v1_5
        algorithms appear in "signature_algorithms".

        ^ Probably the simplest way to verify this is to
        */

        // Given a

        self.check_certificate_verify(&plaintext, cert, &cert_verify, certificate_registry)?;

        Ok(())
    }

    /// Checks that a signature for some plaintext is validly signed by the
    /// given certificate.
    ///
    /// TODO: Make this private.
    pub fn check_certificate_verify(
        &self,
        plaintext: &[u8],
        cert: &x509::Certificate,
        cert_verify: &CertificateVerify,
        certificate_registry: &x509::CertificateRegistry,
    ) -> Result<()> {
        // TODO: Verify this is an algorithm that we requested.

        // Assuming our code is correct, if this fails it should always be the peer's
        // fault as we should have advertised our supported algorithms.
        let (signature_algorithm, constraints) = cert_verify
            .algorithm
            .to_x509_signature_id()
            .ok_or_else(|| err_msg("Unsupported cert verify algorithm"))?;

        let is_valid = cert.public_key(certificate_registry)?.verify_signature(
            plaintext,
            &cert_verify.signature,
            &signature_algorithm,
            &constraints,
        )?;

        if !is_valid {
            return Err(err_msg("Invalid certificate verify signature"));
        }

        Ok(())
    }

    /// Receives a Finished handshake message from the remote endpoint and
    /// verifies that it has the given value.
    pub async fn receive_finished(&mut self, expected_value: &[u8]) -> Result<()> {
        let finished = match self.receive_handshake_message().await? {
            Handshake::Finished(v) => v,
            _ => {
                return Err(err_msg("Expected Finished messages"));
            }
        };

        if !crate::constant_eq(&finished.verify_data, expected_value) {
            return Err(err_msg("Incorrect remote verify_data"));
        }

        Ok(())
    }
}
