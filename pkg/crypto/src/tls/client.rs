use alloc::boxed::Box;
use std::collections::HashMap;
use std::convert::TryInto;
use std::sync::Arc;
use std::vec::Vec;

use common::bytes::Bytes;
use common::errors::*;
use common::io::*;

use crate::tls::application_stream::*;
use crate::tls::cipher_suite::*;
use crate::tls::constants::*;
use crate::tls::extensions::*;
use crate::tls::extensions_util::*;
use crate::tls::handshake::*;
use crate::tls::handshake_executor::HandshakeExecutor;
use crate::tls::handshake_summary::*;
use crate::tls::key_schedule::*;
use crate::tls::key_schedule_helper::*;
use crate::tls::key_schedule_tls12::KeyScheduleTLS12;
use crate::tls::options::ClientOptions;
use crate::tls::record_stream::*;
use crate::x509;

use super::handshake_executor::HandshakeExecutorOptions;

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

impl Client {
    pub fn new() -> Self {
        Self {}
    }

    pub async fn connect(
        &mut self,
        reader: Box<dyn Readable + Sync>,
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

// TODO: Must implement bubbling up Alert messages

/*
Because TLS 1.3 forbids renegotiation, if a server has negotiated
TLS 1.3 and receives a ClientHello at any other time, it MUST
terminate the connection with an "unexpected_message" alert.
*/

enum ServerHelloResult {
    TLS13(ServerHello, KeySchedule),
    TLS12(ServerHello),
}

/// Performs a handshake with a server for a single TLS connection.
/// Implemented from the client point of view.
struct ClientHandshakeExecutor<'a> {
    executor: HandshakeExecutor<'a>,

    options: &'a ClientOptions,

    /// Secrets offered to the server in the last ClientHello sent.
    secrets: HashMap<NamedGroup, Vec<u8>>,

    certificate_registry: x509::CertificateRegistry,

    summary: HandshakeSummary,
}

impl<'a> ClientHandshakeExecutor<'a> {
    async fn new(
        reader: Box<dyn Readable + Sync>,
        writer: Box<dyn Writeable>,
        options: &'a ClientOptions,
    ) -> Result<ClientHandshakeExecutor<'a>> {
        Ok(ClientHandshakeExecutor {
            executor: HandshakeExecutor::new(
                RecordReader::new(reader, false),
                RecordWriter::new(writer, false),
                HandshakeExecutorOptions {
                    is_server: false,
                    local_supported_algorithms: &options.supported_signature_algorithms,
                },
            ),
            options,
            secrets: HashMap::new(),
            certificate_registry: options
                .certificate_request
                .root_certificate_registry
                .resolve()
                .await?
                .child(),
            summary: HandshakeSummary::default(),
        })
    }

    /*
    TODO:
    "If not offering early data, the client sends a dummy
    change_cipher_spec record (see the third paragraph of Section 5)
    immediately before its second flight.  This may either be before
    its second ClientHello or before its encrypted handshake flight.
    If offering early data, the record is placed immediately after the
    first ClientHello."
    */

    async fn run(mut self) -> Result<ApplicationStream> {
        // Send the initial ClientHello message.
        let client_hello = {
            let mut client_shares = vec![];

            for group in self.options.initial_keys_shared.iter().cloned() {
                client_shares.push(self.new_secret(group).await?);
            }

            ClientHello::plain(&client_shares, self.options).await?
        };

        // TODO: Sometimes we should support sending multiple handshake messags in one
        // frame (e.g. for certificate use-cases).

        self.executor
            .send_handshake_message(Handshake::ClientHello(client_hello.clone()))
            .await?;

        let (server_hello, key_schedule) =
            match self.wait_server_hello(client_hello.clone()).await? {
                ServerHelloResult::TLS12(sh) => {
                    self.executor.reader.protocol_version = TLS_1_2_VERSION;
                    return self.run_tls12(client_hello, sh).await;
                }
                ServerHelloResult::TLS13(sh, ks) => {
                    self.executor.reader.protocol_version = TLS_1_3_VERSION;
                    (sh, ks)
                }
            };

        self.process_received_extensions(&server_hello.extensions)?;

        self.wait_encrypted_extensions().await?;

        let mut cert_request = None;
        let cert;
        // Receive optionally a CertificateRequest followed by a Certificate.
        loop {
            match self.executor.receive_handshake_message().await? {
                Handshake::CertificateRequest(req) => {
                    if cert_request.is_some() {
                        return Err(err_msg("Received multiple certificate requests"));
                    }

                    cert_request = Some(req);
                }
                Handshake::Certificate(c) => {
                    cert = self.process_certificate(c).await?;
                    break;
                }
                _ => {
                    return Err(err_msg("Expected certificate message"));
                }
            };
        }

        self.executor
            .receive_certificate_verify_v13(
                &cert,
                key_schedule.hasher_factory(),
                &self.certificate_registry,
            )
            .await?;

        let verify_data_server =
            key_schedule.verify_data_server(&self.executor.handshake_transcript);
        self.executor.receive_finished(&verify_data_server).await?;

        // Should be everything up to server finished.
        let server_finished_secrets =
            key_schedule.server_finished(&self.executor.handshake_transcript);

        if let Some(cert_request) = cert_request {
            let options = match &self.options.certificate_auth {
                Some(v) => v,
                None => {
                    // Lack of the options means that we wouldn't have send the post_handshake_auth
                    // message.
                    return Err(err_msg("Didn't advertise support for certificate auth"));
                }
            };

            let server_supported_algoritms = find_signature_algorithms(&cert_request.extensions)
                .ok_or_else(|| err_msg("Missing supporting algorithms in CR"))?;

            self.executor
                .send_certificate(options, cert_request.certificate_request_context)
                .await?;

            let cert_verify = self
                .executor
                .create_certificate_verify(
                    &key_schedule,
                    &server_supported_algoritms.algorithms,
                    &options.private_key,
                )
                .await?;
            self.executor
                .send_handshake_message(Handshake::CertificateVerify(cert_verify))
                .await?;
        }

        let verify_data_client =
            key_schedule.verify_data_client(&self.executor.handshake_transcript);

        let finished_client = Handshake::Finished(Finished {
            verify_data: verify_data_client,
        });

        self.executor
            .send_handshake_message(finished_client)
            .await?;

        self.executor
            .reader
            .replace_remote_key(server_finished_secrets.server_application_traffic_secret_0)?;

        self.executor
            .writer
            .replace_local_key(server_finished_secrets.client_application_traffic_secret_0)?;

        Ok(ApplicationStream::new(
            self.executor.reader,
            self.executor.writer,
            self.summary,
        ))
    }

    /// Generates a new random secret key and returns the corresponding public
    /// key that can be sent to the server for key exchange.
    async fn new_secret(&mut self, group: NamedGroup) -> Result<KeyShareEntry> {
        let inst = group
            .create()
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

    async fn wait_server_hello(
        &mut self,
        mut client_hello: ClientHello,
    ) -> Result<ServerHelloResult> {
        let mut last_server_hello = None;

        loop {
            let server_hello = {
                if let Handshake::ServerHello(sh) =
                    self.executor.receive_handshake_message().await?
                {
                    sh
                } else {
                    return Err(err_msg("Unexpected message"));
                }
            };

            let is_retry = &server_hello.random == HELLO_RETRY_REQUEST_SHA256;

            // Check that the version is TLS 1.2
            // Then look for a SupportedVersions extension to see if it is TLS 1.3
            let is_tls13 = server_hello.legacy_version == TLS_1_2_VERSION
                && find_supported_versions_sh(&server_hello.extensions)
                    .map(|sv| sv.selected_version == TLS_1_3_VERSION)
                    .unwrap_or(false);
            if !is_tls13 {
                if server_hello.legacy_version == TLS_1_2_VERSION {
                    return Ok(ServerHelloResult::TLS12(server_hello));
                }

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
            if !self
                .options
                .supported_cipher_suites
                .iter()
                .find(|c| **c == server_hello.cipher_suite)
                .is_some()
            {
                return Err(err_msg(
                    "Server selected a cipher suite that we didn't advertise",
                ));
            }

            if is_retry {
                // TODO: A client MUST process all extensions?

                if last_server_hello.is_some() {
                    // TODO: Abort with "unexpected_message"
                    return Err(err_msg("Retrying ClientHello more than once"));
                }

                let selected_group = find_key_share_retry(&server_hello.extensions)
                    .ok_or_else(|| err_msg("Expected key_share in retry"))?
                    .selected_group;

                if self.secrets.contains_key(&selected_group) {
                    return Err(err_msg("Server selected a group that we already picked"));
                }

                if !self
                    .options
                    .supported_groups
                    .iter()
                    .find(|g| **g == selected_group)
                    .is_some()
                {
                    return Err(err_msg("Server selected a group that we didn't advertise"));
                }

                // TODO: See 4.1.2 for retry specific details.
                // In that case, the client MUST send the same
                // ClientHello without modification, except as follows:

                // Remove existing key shares and early_data
                client_hello.extensions.retain(|e| match e {
                    Extension::KeyShareClientHello(_) => false,
                    _ => true,
                });

                // replace shared keys if key_share extension was given
                // NOTE: We clear the secrets from the first client hello as the server
                // shouldn't be able to back track and use a key it initially
                // rejected.
                self.secrets.clear();
                client_hello
                    .extensions
                    .push(Extension::KeyShareClientHello(KeyShareClientHello {
                        client_shares: vec![self.new_secret(selected_group).await?],
                    }));

                // TODO: Remove early_data.

                // Add cookie if given.
                if let Some(cookie) = server_hello.extensions.iter().find(|e| match e {
                    Extension::Cookie(_) => true,
                    _ => false,
                }) {
                    client_hello.extensions.push(cookie.clone());
                }

                // TODO: Update PSK?

                self.executor
                    .send_handshake_message(Handshake::ClientHello(client_hello.clone()))
                    .await?;

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

            let server_public = find_key_share_sh(&server_hello.extensions)
                .ok_or_else(|| err_msg("ServerHello missing key_share"))?;

            if !self.secrets.contains_key(&server_public.server_share.group) {
                return Err(err_msg(
                    "ServerHello key share group not offered in most recent ClientHello",
                ));
            }

            let client_secret = &self.secrets[&server_public.server_share.group];

            let key_schedule = KeyScheduleHelper::create_for_handshake(
                false,
                cipher_suite,
                server_public.server_share.group,
                &server_public.server_share.key_exchange,
                client_secret,
                &self.executor.handshake_transcript,
                &mut self.executor.reader,
                &mut self.executor.writer,
            )?;

            return Ok(ServerHelloResult::TLS13(server_hello, key_schedule));
        }
    }

    async fn wait_encrypted_extensions(&mut self) -> Result<()> {
        let ee = match self.executor.receive_handshake_message().await? {
            Handshake::EncryptedExtensions(e) => e,
            _ => {
                return Err(err_msg("Expected encrypted extensions"));
            }
        };

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
                Extension::ServerName(v) => {
                    if v.is_some() {
                        return Err(err_msg("Server should not return a non-empty server_name"));
                    }
                }
                _ => {}
            }
        }

        Ok(())
    }

    async fn process_certificate(
        &mut self,
        raw_cert: Certificate,
    ) -> Result<Arc<x509::Certificate>> {
        let cert = self
            .executor
            .process_certificate(
                raw_cert,
                &mut self.certificate_registry,
                &self.options.certificate_request,
                Some(&self.options.hostname),
            )
            .await?
            .ok_or_else(|| err_msg("Expected at least one certificate"))?;

        Ok(cert)
    }

    async fn run_tls12(
        mut self,
        client_hello: ClientHello,
        server_hello: ServerHello,
    ) -> Result<ApplicationStream> {
        // TODO: Must verify that the algorithms sent by the server are ok for us to us.

        // TODO: Dedup with the other code that calls this
        self.process_received_extensions(&server_hello.extensions)?;

        let certificate = match self.executor.receive_handshake_message().await? {
            Handshake::Certificate(c) => self.process_certificate(c).await?,
            _ => {
                return Err(err_msg("Expected certificate message"));
            }
        };

        let server_key_exchange = match self.executor.receive_handshake_message().await? {
            Handshake::ServerKeyExchange(c) => c,
            _ => {
                return Err(err_msg("Expected ServerKeyExchange"));
            }
        };

        let server_hello_done = match self.executor.receive_handshake_message().await? {
            Handshake::ServerHelloDone => (),
            _ => {
                return Err(err_msg("Expected ServerKeyExchange"));
            }
        };

        // TODO: Some cipher suites will constrain the type of signature allowed (RSA or
        // ECDSA)
        let server_ecdhe_key = server_key_exchange.ec_diffie_hellman()?;

        // Server should be digitally signing:
        // SHA(ClientHello.random + ServerHello.random +
        //     ServerKeyExchange.params);
        // (for ecdsa algorithms)
        {
            let mut plaintext = vec![];
            plaintext.extend_from_slice(&client_hello.random);
            plaintext.extend_from_slice(&server_hello.random);
            // TODO: Copy this directly out of the original buffer.
            server_ecdhe_key.params.serialize(&mut plaintext);

            self.executor.check_certificate_verify(
                &plaintext,
                &certificate,
                &server_ecdhe_key.signed_params,
                &self.certificate_registry,
            )?;
        }

        let client_pub_key = self
            .new_secret(server_ecdhe_key.params.curve_params.named_curve)
            .await?;

        let mut client_point = vec![];
        ECPoint {
            point: client_pub_key.key_exchange,
        }
        .serialize(&mut client_point);

        self.executor
            .send_handshake_message(Handshake::ClientKeyExchange(ClientKeyExchange {
                data: client_point.into(),
            }))
            .await?;

        self.executor.writer.send_change_cipher_spec().await?;

        let group = client_pub_key.group.create().unwrap();
        let pre_master_secret = group.shared_secret(
            &server_ecdhe_key.params.public.point,
            &self.secrets[&client_pub_key.group],
        )?;

        // TODO: The transcript hash shouldn't include any HelloRequests
        // TODO: Set the transcript's hasher earlier to avoid caching the entire thing.

        let cipher_suite = match server_hello.cipher_suite.decode() {
            Ok(CipherSuiteParts::TLS12(v)) => v,
            _ => {
                return Err(err_msg("Unsupported TLS 1.2 cipher suite"));
            }
        };

        let key_schedule = KeyScheduleTLS12::create(
            cipher_suite,
            &pre_master_secret,
            &client_hello,
            &server_hello,
        );

        self.executor.writer.local_cipher_spec = Some(key_schedule.client_cipher_spec());
        self.executor
            .reader
            .set_remote_cipher_spec(key_schedule.server_cipher_spec())?;

        let verify_data_client =
            key_schedule.verify_data_client(&self.executor.handshake_transcript);

        self.executor
            .send_handshake_message(Handshake::Finished(Finished {
                verify_data: verify_data_client.into(),
            }))
            .await?;

        // TODO: Verify we get a cipher spec message.

        let verify_data_server =
            key_schedule.verify_data_server(&self.executor.handshake_transcript);

        self.executor.receive_finished(&verify_data_server).await?;

        /*
             For TLS 1.2:
             - Decide on cipher using ServerHello
             - Receive Certificate
             - Receive ServerKeyExchange
                 - Vrify Cerificate
             - Receive ServerHelloDone

             - Send ClientKeyExchnage
                 Contains ECPoint serialized with client public key

             - Change cipher spec


             where "pre_master_secret" is the ECDHE secret
                        All ECDH calculations for the NIST curves (including parameter and
        key generation as well as the shared secret calculation) are
        performed according to [IEEE.P1363] using the ECKAS-DH1 scheme with
        the identity map as the Key Derivation Function (KDF) so that the
        premaster secret is the x-coordinate of the ECDH shared secret
        elliptic curve point represented as an octet string.

             - Send Finished

            - Check that we receive a ChangeCipherSpec.

             - Receive Finished
             */

        Ok(ApplicationStream::new(
            self.executor.reader,
            self.executor.writer,
            self.summary,
        ))
    }
}
