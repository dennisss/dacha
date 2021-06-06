use std::collections::VecDeque;
use std::collections::HashMap;
use std::sync::Arc;
use std::convert::TryInto;

use common::async_std::net::TcpStream;
use common::async_std::prelude::*;
use common::async_std::sync::Mutex;
use common::errors::*;
use common::io::*;
use common::bytes::{Buf, Bytes, BytesMut};
use parsing::is_incomplete;


use crate::tls::alert::*;
use crate::tls::extensions::*;
use crate::tls::handshake::*;
use crate::tls::key_schedule::*;
use crate::tls::record::*;
use crate::tls::transcript::*;
use crate::tls::options::ClientOptions;
use crate::aead::*;
use crate::dh::*;
use crate::elliptic::*;
use crate::gcm::*;
use crate::hasher::*;
use crate::hkdf::*;
use crate::sha256::SHA256Hasher;
use crate::sha384::SHA384Hasher;
use crate::tls::cipher::*;
use crate::tls::record_stream::*;
use crate::tls::application_stream::*;
use crate::x509;
use crate::tls::handshake_summary::*;
use crate::chacha20::ChaCha20Poly1305;


// TODO: Should abort the connection if negotiation results in more than one
// retry as the first retry should always have enough information.

pub struct Client {
    // hasher

// diffie helman private key

// transcript of handshake messages (do i also need alerts?)

// pending partially updated

// cookie (to be validated and passed in the next )

// /// Messages that have been received but haven't yet been processed.
// pending_messages: VecDeque<Message>
}

fn find_supported_versions_sh(
    extensions: &Vec<Extension>,
) -> Option<&SupportedVersionsServerHello> {
    for e in extensions {
        if let Extension::SupportedVersionsServerHello(v) = e {
            return Some(v);
        }
    }

    None
}

fn find_key_share_sh(extensions: &Vec<Extension>) -> Option<&KeyShareServerHello> {
    for e in extensions {
        if let Extension::KeyShareServerHello(v) = e {
            return Some(v);
        }
    }

    None
}

fn find_key_share_retry(extensions: &[Extension]) -> Option<&KeyShareHelloRetryRequest> {
    for e in extensions {
        if let Extension::KeyShareHelloRetryRequest(v) = e {
            return Some(v);
        }
    }

    None
}

// TODO: Must implement bubbling up Alert messages

// TODO: Unused?
const TLS13_CERTIFICATEVERIFY_CLIENT_CTX: &'static [u8] = b"TLS 1.3, client CertificateVerify";
const TLS13_CERTIFICATEVERIFY_SERVER_CTX: &'static [u8] = b"TLS 1.3, server CertificateVerify";

/// SHA-256 of the string "HelloRetryRequest" which indicates that a ServerHello is a
/// HelloRetryRequest.
const HELLO_RETRY_REQUEST_SHA256: &'static [u8] = &[
    0xCF, 0x21, 0xAD, 0x74, 0xE5, 0x9A, 0x61, 0x11, 0xBE, 0x1D, 0x8C, 0x02, 0x1E, 0x65, 0xB8, 0x91,
    0xC2, 0xA2, 0x11, 0x16, 0x7A, 0xBB, 0x8C, 0x5E, 0x07, 0x9E, 0x09, 0xE2, 0xC8, 0xA8, 0x33, 0x9C
];

/*
Because TLS 1.3 forbids renegotiation, if a server has negotiated
TLS 1.3 and receives a ClientHello at any other time, it MUST
terminate the connection with an "unexpected_message" alert.
*/


/// Performs a handshake with a server for a single TLS connection.
/// Implemented from the client point of view.
struct ClientHandshakeExecutor<'a> {
    options: &'a ClientOptions,
    
    reader: RecordReader,
    writer: RecordWriter,

    handshake_transcript: Transcript,

    /// Secrets offered to the server in the last ClientHello sent.
    secrets: HashMap<NamedGroup, Vec<u8>>,

    certificate_registry: x509::CertificateRegistry,

    summary: HandshakeSummary
}

impl<'a> ClientHandshakeExecutor<'a> {
    async fn new(
        reader: Box<dyn Readable>, writer: Box<dyn Writeable>,
        options: &'a ClientOptions
    ) -> Result<ClientHandshakeExecutor<'a>> {
        Ok(ClientHandshakeExecutor {
            options,
            reader: RecordReader::new(reader),
            writer: RecordWriter::new(writer),
            handshake_transcript: Transcript::new(),
            secrets: HashMap::new(),
            certificate_registry: x509::CertificateRegistry::public_roots().await?,
            summary: HandshakeSummary::default()
        })
    }

    async fn run(mut self) -> Result<ApplicationStream> {
        // Send the initial ClientHello message.
        let client_hello = {
            let mut client_shares = vec![];

            for group in self.options.initial_keys_shared.iter().cloned() {
                client_shares.push(self.new_secret(group).await?);
            }

            ClientHello::plain(&client_shares, self.options).await?   
        };

        // TODO: Sometimes we should support sending multiple handshake messags in one frame
        // (e.g. for certificate use-cases).

        self.writer.send_handshake(
            Handshake::ClientHello(client_hello.clone()), Some(&mut self.handshake_transcript))
            .await?;

        println!("WAIT_SH");

        let (server_hello, key_schedule, hasher_factory) =
            self.wait_server_hello(client_hello).await?;
        self.process_received_extensions(&server_hello.extensions)?;

        println!("WAIT_EE");
        self.wait_encrypted_extensions().await?;

        // TODO: Could receive either CertificateRequest or Certificate

        println!("WAIT_CERT");
        let cert = self.wait_certificate().await?;

        self.wait_certificate_verify(&cert, &hasher_factory).await?;
        
        println!("WAIT_FINISHED");

        self.wait_finished(key_schedule).await?;

        println!("DONE");

        Ok(ApplicationStream::new(self.reader, self.writer, self.summary))
    }

    /// Generates a new random secret key and returns the corresponding public key
    /// that can be sent to the server for key exchange.
    async fn new_secret(&mut self, group: NamedGroup) -> Result<KeyShareEntry> {
        let inst = group.create()
            .ok_or_else(|| err_msg("NamedGroup not supported"))?;
        
        let secret_value = inst.secret_value().await?;
        let entry = KeyShareEntry {
            group,
            key_exchange: inst.public_value(&secret_value)?.into(),
        };

        // TODO: Verify no duplicates.
        self.secrets.insert(group, secret_value);

        Ok(entry)
    }

    async fn receive_handshake_message(&mut self) -> Result<Handshake> {
        loop {
            let msg = self.reader.recv(Some(&mut self.handshake_transcript)).await?;

            match msg {
                Message::Handshake(m) => { return Ok(m); },
                Message::ApplicationData(_) => {
                    return Err(err_msg("Unexpected application data during handshake"));
                },
                Message::ChangeCipherSpec(_) => {
                    // TODO: Improve this.
                    continue;
                },
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

    async fn wait_server_hello(
        &mut self, mut client_hello: ClientHello
    ) -> Result<(ServerHello, KeySchedule, HasherFactory)> {
        let mut last_server_hello = None;

        loop {
            let server_hello = {
                if let Handshake::ServerHello(sh) = self.receive_handshake_message().await? { sh }
                else { return Err(err_msg("Unexpected message")); }
            };

            println!("{:#?}", server_hello);

            let is_retry = &server_hello.random == HELLO_RETRY_REQUEST_SHA256;

            // Check that the version is TLS 1.2
            // Then look for a SupportedVersions extension to see if it is TLS 1.3
            let is_tls13 = server_hello.legacy_version == TLS_1_2_VERSION
                && find_supported_versions_sh(&server_hello.extensions)
                    .map(|sv| sv.selected_version == TLS_1_3_VERSION)
                    .unwrap_or(false);
            if !is_tls13 {

                // TODO: If we switch to TLS 1.2, make sure that we don't allow retrying again.
                // ^ Also if this isn't a retry request, then we should complain??

                return Err(err_msg("Only support TLS 1.3"));
            }

            // TODO: Must match ClientHello?
            if server_hello.legacy_compression_method != 0 {
                return Err(err_msg("Unexpected compression method"));
            }

            // TODO: Check legacy_session_id_echo

            // TODO: Must check the random bytes received.

            // Verify that the cipher suite was offered.
            if !self.options.supported_cipher_suites.iter().find(
                |c| **c == server_hello.cipher_suite).is_some() {
                return Err(err_msg("Server selected a cipher suite that we didn't advertise"));
            } 

            if is_retry {
                // TODO: A client MUST process all extensions?

                if last_server_hello.is_some() {
                    // TODO: Abort with "unexpected_message"
                    return Err(err_msg("Retrying ClientHello more than once"));
                }

                let selected_group =
                    find_key_share_retry(&server_hello.extensions)
                    .ok_or_else(|| err_msg("Expected key_share in retry"))?
                    .selected_group;

                if self.secrets.contains_key(&selected_group) {
                    return Err(err_msg("Server selected a group that we already picked"));
                }

                if !self.options.supported_groups.iter().find(|g| **g == selected_group).is_some() {
                    return Err(err_msg("Server selected a group that we didn't advertise"));
                }


                // TODO: See 4.1.2 for retry specific details.
                // In that case, the client MUST send the same
                // ClientHello without modification, except as follows:

                // Remove existing key shares and early_data
                client_hello.extensions.retain(|e| {
                    match e {
                        Extension::KeyShareClientHello(_) => false,
                        _ => true
                    }
                });

                // replace shared keys if key_share extension was given
                // NOTE: We clear the secrets from the first client hello as the server shouldn't be able to
                // back track and use a key it initially rejected.
                self.secrets.clear();
                client_hello.extensions.push(Extension::KeyShareClientHello(KeyShareClientHello {
                    client_shares: vec![
                        self.new_secret(selected_group).await?
                    ]
                }));


                // TODO: Remove early_data.

                // Add cookie if given.
                if let Some(cookie) = server_hello.extensions.iter().find(|e| {
                    match e {
                        Extension::Cookie(_) => true,
                        _ => false
                    }
                }) {
                    client_hello.extensions.push(cookie.clone());
                }

                // TODO: Update PSK?

                self.writer.send_handshake(
                    Handshake::ClientHello(client_hello.clone()), Some(&mut self.handshake_transcript)).await?;

                // Wait for the ServerHello again.
                last_server_hello = Some(server_hello);
                continue;
            }

            if let Some(hello_retry) = &last_server_hello {
                if hello_retry.cipher_suite != server_hello.cipher_suite {
                    // TODO: Abort with "illegal_parameter"
                    return Err(err_msg("cipher_suite changed after retry"));
                }
            }

            let cipher_suite = server_hello.cipher_suite.clone();

            let (aead, hasher_factory): (Box<dyn AuthEncAD>, HasherFactory) = match cipher_suite {
                CipherSuite::TLS_AES_128_GCM_SHA256 => {
                    (Box::new(AesGCM::aes128()), SHA256Hasher::factory())
                }
                CipherSuite::TLS_AES_256_GCM_SHA384 => {
                    (Box::new(AesGCM::aes256()), SHA384Hasher::factory())
                }
                CipherSuite::TLS_CHACHA20_POLY1305_SHA256 => {
                	(Box::new(ChaCha20Poly1305::new()), SHA256Hasher::factory())
                },
                _ => {
                    return Err(err_msg("Bad cipher suite"));
                }
            };

            let hkdf = HKDF::new(hasher_factory.box_clone());

            let mut key_schedule = KeySchedule::new(hkdf.clone(), hasher_factory.box_clone());

            key_schedule.early_secret(None);

            let server_public = find_key_share_sh(&server_hello.extensions)
                .ok_or(err_msg("ServerHello missing key_share"))?;

            if !self.secrets.contains_key(&server_public.server_share.group) {
                return Err(err_msg("ServerHello key share group not offered in most recent ClientHello"));
            }

            let group = server_public.server_share.group.create().unwrap();

            let shared_secret =
                group.shared_secret(&server_public.server_share.key_exchange,
                    &self.secrets[&server_public.server_share.group])?;

            // TODO: Use this return value? 
            key_schedule.handshake_secret(&shared_secret);

            let (client_handshake_traffic_secret, server_handshake_traffic_secret) = {
                let s = key_schedule.handshake_traffic_secrets(&self.handshake_transcript);
                (
                    s.client_handshake_traffic_secret,
                    s.server_handshake_traffic_secret,
                )
            };

            // TODO: Use this?
            key_schedule.master_secret();


            self.writer.local_cipher_spec = Some(CipherEndpointSpec::new(
                aead.box_clone(),
                hkdf.clone(),
                client_handshake_traffic_secret.into()
            ));

            self.reader.set_remote_cipher_spec(CipherEndpointSpec::new(
                aead.box_clone(),
                hkdf.clone(),
                server_handshake_traffic_secret.into()
            ))?;

            return Ok((server_hello, key_schedule, hasher_factory));
        }
    }
    
    async fn wait_encrypted_extensions(&mut self) -> Result<()> {
        let ee = match self.receive_handshake_message().await? {
            Handshake::EncryptedExtensions(e) => e,
            _ => {
                return Err(err_msg("Expected encrypted extensions"));
            }
        };

        println!("{:#?}", ee);

        self.process_received_extensions(&ee.extensions)?;

        // TODO: Process all of these extensions.

        Ok(())
    }

    fn process_received_extensions(&mut self, extensions: &[Extension]) -> Result<()> {
        for e in extensions {
            match e {
                Extension::ALPN(protocols) => {
                    if protocols.names.len() != 1 || self.summary.selected_alpn_protocol.is_some() {
                        return Err(err_msg("Expected to get exactly one ALPN selection"));
                    }

                    self.summary.selected_alpn_protocol = Some(protocols.names[0].clone());
                }
                _ => {}
            }
        }

        Ok(())
    }

    /// Receives the server's certificate. Responsible for verifying that:
    /// 1. The certificate is valid now
    /// 2. The certificate is valid for the remote host name.
    /// 3. The certificate forms a valid chain to a trusted root certificate.
    async fn wait_certificate(&mut self) -> Result<Arc<x509::Certificate>> {
        let cert = match self.receive_handshake_message().await? {
            Handshake::Certificate(c) => c,
            _ => {
                return Err(err_msg("Expected certificate message"));
            }
        };

        if cert.certificate_request_context.len() != 0 {
            return Err(err_msg("Unexpected request context width certificate"));
        }

        let mut cert_list = vec![];
        for c in &cert.certificate_list {
            cert_list.push(Arc::new(x509::Certificate::read(
                c.cert.clone(),
            )?));
        }

        if cert_list.len() < 1 {
            return Err(err_msg("Expected at least one certificate"));
        }

        // NOTE: This will return an error if any of the certificates are invalid.
        // TODO: Technically we only need to ensure that the first one is valid.
        self.certificate_registry.append(
            &cert_list, self.options.trust_server_certificate)?;

        // Verify the terminal certificate is valid (MUST always be first).

        if !cert_list[0].valid_now() {
            return Err(err_msg("Certificate not valid now"));
        }

        if let Some(usage) = cert_list[0].key_usage()? {
            if !usage.digitalSignature().unwrap_or(false) {
                return Err(err_msg(
                    "Certificate can't be used for signature verification",
                ));
            }
        }

        // TODO: Remove the trust_server_certificate exception and instead always check this.
        if !self.options.trust_server_certificate {
            if !cert_list[0].for_dns_name(&self.options.hostname)? {
                return Err(err_msg("Certificate not valid for DNS name"));
            }
        }

        Ok(cert_list.remove(0))
    }

    async fn wait_certificate_verify(&mut self, cert: &x509::Certificate, hasher_factory: &HasherFactory) -> Result<()> {
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
        plaintext.extend_from_slice(&TLS13_CERTIFICATEVERIFY_SERVER_CTX);
        plaintext.push(0);
        plaintext.extend_from_slice(&ch_ct_transcript_hash);

        if self.options.supported_signature_algorithms.iter().find(|a| **a == cert_verify.algorithm).is_none() {
            // TODO: This may happen if no certificate exists that can be used with any of the requested
            // algorithms.
            return Err(err_msg("Received certificate verification with non-advertised algorithm."));
        }

        /*
        TODO:
        For TLS 1.3:
        RSA signatures MUST use an RSASSA-PSS algorithm, regardless of whether RSASSA-PKCS1-v1_5
        algorithms appear in "signature_algorithms".
        */

        // TODO: Verify this is an algorithm that we requested (and that it
        // matches all relevant params in the certificate.
        // TOOD: Most of this code should be easy to modularize.
        match cert_verify.algorithm {
            SignatureScheme::ecdsa_secp256r1_sha256 => {
                let (params, point) = cert.ec_public_key(&self.certificate_registry)?;
                let group = EllipticCurveGroup::secp256r1();

                if params != group {
                    return Err(err_msg("Mismatch between signature and public key algorithm!!"));
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
                let rsa = crate::rsa::RSASSA_PSS::new(
                    crate::sha256::SHA256Hasher::factory(), 256 / 8);
                
                let good = rsa.verify_signature(&public_key.try_into()?, &cert_verify.signature, &plaintext)?;
                if !good {
                    return Err(err_msg("Invalid RSA certificate verify signature"));
                }
            }

            // TODO:
            // SignatureScheme::rsa_pkcs1_sha256,
            // SignatureScheme::rsa_pss_rsae_sha256
            _ => {
                return Err(err_msg("Unsupported cert verify algorithm"));
            }
        };

        Ok(())
    }

    async fn wait_finished(&mut self, key_schedule: KeySchedule) -> Result<()> {
        let verify_data_server = key_schedule.verify_data_server(&self.handshake_transcript);

        let finished = match self.reader.recv(Some(&mut self.handshake_transcript)).await? {
            Message::Handshake(Handshake::Finished(v)) => v,
            _ => {
                return Err(err_msg("Expected Finished messages"));
            }
        };

        if finished.verify_data != verify_data_server {
            return Err(err_msg("Incorrect server verify_data"));
        }

        let verify_data_client = key_schedule.verify_data_client(&self.handshake_transcript);

        let finished_client = Handshake::Finished(Finished {
            verify_data: verify_data_client,
        });

        let final_secrets = key_schedule.finished(&self.handshake_transcript);

        self.writer
            .send_handshake(finished_client, Some(&mut self.handshake_transcript))
            .await?;

        self.reader
            .replace_remote_key(final_secrets.server_application_traffic_secret_0)?;

        self.writer
            .local_cipher_spec.as_mut().unwrap()
            .replace_key(final_secrets.client_application_traffic_secret_0);

        Ok(())
    }


}


impl Client {
    pub fn new() -> Self {
        Self {}
    }

    pub async fn connect(
        &mut self,
        reader: Box<dyn Readable>,
        writer: Box<dyn Writeable>,
        options: &ClientOptions,
    ) -> Result<ApplicationStream> {
        let handshake_exec = ClientHandshakeExecutor::new(reader, writer, options).await?;
        handshake_exec.run().await
        

        //////

        // TODO: Validate that all extensions have been interprated in some way.

        // If 1.3, check the selected group is one of the ones that we wanted

        // Assert that there is a key_share in the ServerHello

        // Decode the cipher suite in order to at least start using the right
        // hash.

        // Generate the shared secret

        // AES AEAD stuff is here: https://tools.ietf.org/html/rfc5116
        // https://tools.ietf.org/html/rfc5288

        // ChaCha20 in here: https://tools.ietf.org/html/rfc8439

        // println!("RES: {:?}", &res_data[0..n]);
    }
}
