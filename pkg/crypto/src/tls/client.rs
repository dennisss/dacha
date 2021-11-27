use std::collections::HashMap;
use std::convert::TryInto;
use std::sync::Arc;

use common::bytes::Bytes;
use common::errors::*;
use common::io::*;

use crate::aead::AuthEncAD;
use crate::elliptic::*;
use crate::gcm::AesGCM;
use crate::hasher::*;
use crate::sha256::SHA256Hasher;
use crate::tls::alert::*;
use crate::tls::application_stream::*;
use crate::tls::cipher::CipherEndpointSpec;
use crate::tls::cipher_suite::*;
use crate::tls::cipher_tls12::CipherEndpointSpecTLS12;
use crate::tls::cipher_tls12::GCMNonceGenerator;
use crate::tls::cipher_tls12::NonceGenerator;
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
use crate::tls::transcript::*;
use crate::x509;

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
    executor: HandshakeExecutor,

    options: &'a ClientOptions,

    handshake_transcript: Transcript,

    /// Secrets offered to the server in the last ClientHello sent.
    secrets: HashMap<NamedGroup, Vec<u8>>,

    certificate_registry: x509::CertificateRegistry,

    summary: HandshakeSummary,
}

impl<'a> ClientHandshakeExecutor<'a> {
    async fn new(
        reader: Box<dyn Readable>,
        writer: Box<dyn Writeable>,
        options: &'a ClientOptions,
    ) -> Result<ClientHandshakeExecutor<'a>> {
        Ok(ClientHandshakeExecutor {
            executor: HandshakeExecutor::new(
                RecordReader::new(reader, false),
                RecordWriter::new(writer, false),
            ),
            options,
            handshake_transcript: Transcript::new(),
            secrets: HashMap::new(),
            certificate_registry: x509::CertificateRegistry::public_roots().await?,
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

        println!("WAIT_SH");

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

        println!("WAIT_EE");
        self.wait_encrypted_extensions().await?;

        // TODO: Could receive either CertificateRequest or Certificate

        println!("WAIT_CERT");
        let cert = self.receive_certificate().await?;

        self.executor
            .receive_certificate_verify_v13(
                false,
                &cert,
                key_schedule.hasher_factory(),
                &self.options.supported_signature_algorithms,
                &self.certificate_registry,
            )
            .await?;

        println!("WAIT_FINISHED");

        self.wait_finished(key_schedule).await?;

        println!("DONE");

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

            println!("{:#?}", server_hello);

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
                &self.handshake_transcript,
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

    /// Receives the client/server's Certificate and verifies that:
    /// 1. The certificate is valid now
    /// 2. The certificate is valid for the remote host name.
    /// 3. The certificate forms a valid chain to a trusted root certificate.
    ///
    /// This is both TLS 1.2 and 1.3.
    async fn receive_certificate(
        &mut self,
        // certificate_registry: &mut x509::CertificateRegistry,
    ) -> Result<Arc<x509::Certificate>> {
        let cert = match self.executor.receive_handshake_message().await? {
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
            cert_list.push(Arc::new(x509::Certificate::read(c.cert.clone())?));
        }

        if cert_list.len() < 1 {
            return Err(err_msg("Expected at least one certificate"));
        }

        // NOTE: This will return an error if any of the certificates are invalid.
        // TODO: Technically we only need to ensure that the first one is valid.
        self.certificate_registry
            .append(&cert_list, self.options.trust_server_certificate)?;

        // Verify the terminal certificate is valid (MUST always be first).

        // TODO: How do we verify that all parent certificates are allowed to issue
        // sub-certificates.
        // - Must validate 'Certificate Basic Constraints' and 'Certificate Key Usage'
        //   to verify that certificates can be signed.

        if let Some(usage) = cert_list[0].key_usage()? {
            if !usage.digitalSignature().unwrap_or(false) {
                return Err(err_msg(
                    "Certificate can't be used for signature verification",
                ));
            }
        }

        // TODO: Remove the trust_server_certificate exception and instead always check
        // this.
        if !self.options.trust_server_certificate {
            if !cert_list[0].valid_now() {
                return Err(err_msg("Certificate not valid now"));
            }

            if !cert_list[0].for_dns_name(&self.options.hostname)? {
                return Err(err_msg("Certificate not valid for DNS name"));
            }
        }

        Ok(cert_list.remove(0))
    }

    async fn wait_finished(&mut self, key_schedule: KeySchedule) -> Result<()> {
        let verify_data_server =
            key_schedule.verify_data_server(&self.executor.handshake_transcript);

        // TODO: Split to using recv_handshake_message
        let finished = match self.executor.receive_handshake_message().await? {
            Handshake::Finished(v) => v,
            _ => {
                return Err(err_msg("Expected Finished messages"));
            }
        };

        if finished.verify_data != verify_data_server {
            return Err(err_msg("Incorrect server verify_data"));
        }

        let verify_data_client =
            key_schedule.verify_data_client(&self.executor.handshake_transcript);

        let finished_client = Handshake::Finished(Finished {
            verify_data: verify_data_client,
        });

        // Should be everything up to server finished.
        let final_secrets = key_schedule.finished(&self.executor.handshake_transcript);

        self.executor
            .send_handshake_message(finished_client)
            .await?;

        self.executor
            .reader
            .replace_remote_key(final_secrets.server_application_traffic_secret_0)?;

        self.executor
            .writer
            .replace_local_key(final_secrets.client_application_traffic_secret_0)?;

        Ok(())
    }

    async fn run_tls12(
        mut self,
        client_hello: ClientHello,
        server_hello: ServerHello,
    ) -> Result<ApplicationStream> {
        println!("PROCESS TLS1.2");

        // TODO: Must verify that the algorithms sent by the server are ok for us to us.

        // TODO: Dedup with the other code that calls this
        self.process_received_extensions(&server_hello.extensions)?;

        println!("WAIT 1.2 CERT");

        let certificate = self.receive_certificate().await?;

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
        println!("SKE: {:#?}", server_ecdhe_key);

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

        let server_finished = match self.executor.receive_handshake_message().await? {
            Handshake::Finished(v) => v,
            _ => {
                return Err(err_msg("Expected Finished"));
            }
        };
        println!("{:#?}", server_finished);

        if &server_finished.verify_data != &verify_data_server {
            return Err(err_msg("Bad server finished"));
        }

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
