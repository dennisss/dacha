use std::convert::TryInto;

use common::errors::*;
use pkix::PKIX1Algorithms2008;

use crate::elliptic::EllipticCurveGroup;
use crate::hasher::{GetHasherFactory, HasherFactory};
use crate::tls::alert::AlertLevel;
use crate::tls::constants::*;
use crate::tls::extensions::SignatureScheme;
use crate::tls::handshake::{CertificateVerify, Handshake};
use crate::tls::key_schedule::KeySchedule;
use crate::tls::record_stream::{Message, RecordReader, RecordWriter};
use crate::tls::transcript::Transcript;
use crate::x509;

const TLS13_CERTIFICATEVERIFY_CLIENT_CTX: &'static [u8] = b"TLS 1.3, client CertificateVerify";
const TLS13_CERTIFICATEVERIFY_SERVER_CTX: &'static [u8] = b"TLS 1.3, server CertificateVerify";

/// Common interface for executing client/server TLS handshakes.
///
/// NOTE: This is an internal interface to be primarily used by the 'client' and
/// 'server' modules.
pub(super) struct HandshakeExecutor {
    pub reader: RecordReader,
    pub writer: RecordWriter,
    pub handshake_transcript: Transcript,
}

impl HandshakeExecutor {
    pub fn new(reader: RecordReader, writer: RecordWriter) -> Self {
        Self {
            reader,
            writer,
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

    /// Creates a TLS 1.3 CertificateVerify message to be sent after a
    /// Certificate.
    pub async fn create_certificate_verify(
        &self,
        is_server: bool,
        key_schedule: &KeySchedule,
        local_supported_algorithms: &[SignatureScheme],
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
        plaintext.extend_from_slice(if is_server {
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
        for algorithm in local_supported_algorithms {
            if !remote_supported_algorithms.contains(algorithm) {
                continue;
            }

            let compatible = match private_key {
                x509::PrivateKey::RSA(_) => {
                    // TODO: Better distinguish between the two types of RSA.
                    match algorithm {
                        SignatureScheme::rsa_pkcs1_sha256
                        | SignatureScheme::rsa_pkcs1_sha384
                        | SignatureScheme::rsa_pss_rsae_sha256
                        | SignatureScheme::rsa_pss_rsae_sha384
                        | SignatureScheme::rsa_pss_rsae_sha512
                        | SignatureScheme::rsa_pss_pss_sha256
                        | SignatureScheme::rsa_pss_pss_sha384
                        | SignatureScheme::rsa_pss_pss_sha512
                        | SignatureScheme::rsa_pkcs1_sha1 => true,
                        _ => false,
                    }
                }
                x509::PrivateKey::ECDSA(group, _, _) => match algorithm {
                    SignatureScheme::ecdsa_secp256r1_sha256 => {
                        *group == PKIX1Algorithms2008::SECP256R1
                    }
                    SignatureScheme::ecdsa_secp384r1_sha384 => {
                        *group == PKIX1Algorithms2008::SECP384R1
                    }
                    SignatureScheme::ecdsa_secp521r1_sha512 => {
                        *group == PKIX1Algorithms2008::SECP521R1
                    }
                    _ => false,
                },
            };

            if !compatible {
                continue;
            }

            selected_signature_algorithm = Some(algorithm.clone());
            break;
        }

        let selected_signature_algorithm = selected_signature_algorithm
            .ok_or_else(|| err_msg("Failed to get a good algorithm"))?;

        match selected_signature_algorithm {
            SignatureScheme::ecdsa_secp256r1_sha256 => {
                // TODO: Check that the group is SECP256R1
                let (_, group, private_key) = match private_key {
                    x509::PrivateKey::ECDSA(a, b, c) => (a, b, c),
                    _ => {
                        return Err(err_msg("Wrong private key format"));
                    }
                };

                let mut hasher = crate::sha256::SHA256Hasher::default();

                let signature = group
                    .create_signature(&private_key, &plaintext, &mut hasher)
                    .await?;

                return Ok(CertificateVerify {
                    algorithm: selected_signature_algorithm,
                    signature: signature.into(),
                });
            }
            SignatureScheme::rsa_pss_rsae_sha256 => {
                let private_key = match private_key {
                    x509::PrivateKey::RSA(key) => key,
                    _ => {
                        return Err(err_msg("Wrong private key format"));
                    }
                };

                // NOTE: Salt length should be the same as the digest/hash length.
                let rsa =
                    crate::rsa::RSASSA_PSS::new(crate::sha256::SHA256Hasher::factory(), 256 / 8);

                let signature = rsa.create_signature(private_key, &plaintext).await?;

                return Ok(CertificateVerify {
                    algorithm: selected_signature_algorithm,
                    signature: signature.into(),
                });
            }
            SignatureScheme::rsa_pkcs1_sha256 => {
                // TODO: This shouldn't be used in TLS 1.3

                let private_key = match private_key {
                    x509::PrivateKey::RSA(key) => key,
                    _ => {
                        return Err(err_msg("Wrong private key format"));
                    }
                };

                let rsa = crate::rsa::RSASSA_PKCS_v1_5::sha256();

                let signature = rsa.create_signature(private_key, &plaintext)?;

                return Ok(CertificateVerify {
                    algorithm: selected_signature_algorithm,
                    signature: signature.into(),
                });
            }
            _ => {
                return Err(err_msg("Unsupported cert verify algorithm"));
            }
        };
    }

    /// Receives a TLS 1.3 CertificateVerify message from a remote client or
    /// server and verifies that it is valid.
    pub async fn receive_certificate_verify_v13(
        &mut self,
        is_server: bool,
        cert: &x509::Certificate,
        hasher_factory: &HasherFactory,
        local_supported_algorithms: &[SignatureScheme],
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
        plaintext.extend_from_slice(if is_server {
            &TLS13_CERTIFICATEVERIFY_CLIENT_CTX
        } else {
            &TLS13_CERTIFICATEVERIFY_SERVER_CTX
        });
        plaintext.push(0);
        plaintext.extend_from_slice(&ch_ct_transcript_hash);

        if local_supported_algorithms
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
    pub fn check_certificate_verify(
        &self,
        plaintext: &[u8],
        cert: &x509::Certificate,
        cert_verify: &CertificateVerify,
        certificate_registry: &x509::CertificateRegistry,
    ) -> Result<()> {
        // TODO: Move more of this code into the certificate class.

        // TODO: Verify this is an algorithm that we requested (and that it
        // matches all relevant params in the certificate.
        // TOOD: Most of this code should be easy to modularize.
        match cert_verify.algorithm {
            SignatureScheme::ecdsa_secp256r1_sha256 => {
                let (params, point) = cert.ec_public_key(certificate_registry)?;
                let group = EllipticCurveGroup::secp256r1();

                if params != group {
                    return Err(err_msg(
                        "Mismatch between signature and public key algorithm!!",
                    ));
                }

                let mut hasher = crate::sha256::SHA256Hasher::default();
                let good = group.verify_signature(
                    point.as_ref(),
                    &cert_verify.signature,
                    &plaintext,
                    &mut hasher,
                )?;
                if !good {
                    return Err(err_msg("Invalid ECSDA certificate verify signature"));
                }
            }
            SignatureScheme::rsa_pss_rsae_sha256 => {
                // NOTE: Salt length should be the same as the digest/hash length.
                let public_key = cert.rsa_public_key()?;
                let rsa =
                    crate::rsa::RSASSA_PSS::new(crate::sha256::SHA256Hasher::factory(), 256 / 8);

                let good = rsa.verify_signature(
                    &public_key.try_into()?,
                    &cert_verify.signature,
                    &plaintext,
                )?;
                if !good {
                    return Err(err_msg("Invalid RSA certificate verify signature"));
                }
            }
            SignatureScheme::rsa_pkcs1_sha512 => {
                let public_key = cert.rsa_public_key()?;
                let rsa = crate::rsa::RSASSA_PKCS_v1_5::sha512();

                let good = rsa.verify_signature(
                    &public_key.try_into()?,
                    &cert_verify.signature,
                    &plaintext,
                )?;

                if !good {
                    return Err(err_msg("Invalid RSA PKCS certificate verify signature"));
                }
            }

            // TODO:
            // SignatureScheme::rsa_pkcs1_sha256,
            _ => {
                return Err(err_msg("Unsupported cert verify algorithm"));
            }
        };

        Ok(())
    }
}
