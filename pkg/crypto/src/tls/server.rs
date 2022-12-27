use alloc::boxed::Box;

use common::bytes::Bytes;
use common::errors::*;
use common::io::{Readable, Writeable};

use crate::random::secure_random_bytes;
use crate::tls::application_stream::ApplicationStream;
use crate::tls::cipher_suite::*;
use crate::tls::extensions::*;
use crate::tls::extensions_util::*;
use crate::tls::handshake::ClientHello;
use crate::tls::handshake::{
    CertificateRequest, EncryptedExtensions, Finished, Handshake, ServerHello, TLS_1_2_VERSION,
    TLS_1_3_VERSION,
};
use crate::tls::handshake_executor::{HandshakeExecutor, HandshakeExecutorOptions};
use crate::tls::handshake_summary::HandshakeSummary;
use crate::tls::key_schedule_helper::KeyScheduleHelper;
use crate::tls::options::ServerOptions;
use crate::tls::record_stream::{RecordReader, RecordWriter};
use crate::x509;

pub struct Server {}

impl Server {
    pub async fn connect(
        reader: Box<dyn Readable + Sync>,
        writer: Box<dyn Writeable>,
        options: &ServerOptions,
    ) -> Result<ApplicationStream> {
        let executor = ServerHandshakeExecutor::create(reader, writer, options).await?;
        executor.run().await
    }
}

struct ServerHandshakeExecutor<'a> {
    executor: HandshakeExecutor<'a>,
    options: &'a ServerOptions,
    summary: HandshakeSummary,
    /// If performing a certificate request, this will be present.
    certificate_registry: Option<x509::CertificateRegistry>,
}

impl<'a> ServerHandshakeExecutor<'a> {
    pub async fn create(
        reader: Box<dyn Readable + Sync>,
        writer: Box<dyn Writeable>,
        options: &'a ServerOptions,
    ) -> Result<ServerHandshakeExecutor<'a>> {
        let mut certificate_registry = None;
        if let Some(options) = &options.certificate_request {
            certificate_registry = Some(options.root_certificate_registry.resolve().await?.child());
        }

        Ok(Self {
            executor: HandshakeExecutor::new(
                RecordReader::new(reader, true),
                RecordWriter::new(writer, true),
                HandshakeExecutorOptions {
                    is_server: true,
                    local_supported_algorithms: &options.supported_signature_algorithms,
                },
            ),
            options,
            summary: HandshakeSummary::default(),
            certificate_registry,
        })
    }

    /// TODO: Pick a better name for this?
    /// TODO: Filter out io errors.
    pub async fn run(mut self) -> Result<ApplicationStream> {
        let client_hello = match self.executor.receive_handshake_message().await? {
            Handshake::ClientHello(v) => v,
            // TODO: Send an alert?
            _ => return Err(err_msg("Expected ClientHello")),
        };

        let client_supported_versions = find_supported_versions_ch(&client_hello.extensions)
            .ok_or_else(|| err_msg("No supported versions"))?;

        if !client_supported_versions
            .versions
            .contains(&TLS_1_3_VERSION)
        {
            return Err(err_msg("Client doesn't supported TLS 1.3"));
        }

        self.executor.reader.protocol_version = TLS_1_3_VERSION;

        let client_key_share_ext = find_key_share_ch(&client_hello.extensions)
            .ok_or_else(|| err_msg("Expected client key share"))?;

        let client_signature_algorithms_ext =
            find_signature_algorithms(&client_hello.extensions)
                .ok_or_else(|| err_msg("Expected client supported algorithms"))?;

        // TODO: Verify that we weren't given a pre-shared key or early data

        let client_key_share = {
            let mut selected_client_share = None;
            for client_share in &client_key_share_ext.client_shares {
                if !self.options.supported_groups.contains(&client_share.group) {
                    continue;
                }

                selected_client_share = Some(client_share);
                break;
            }

            // TODO: In this case, send a retry request.
            selected_client_share
                .ok_or_else(|| err_msg("No supported key share in client hello"))?
        };

        let (server_secret, server_public) = {
            let group = client_key_share.group.create().unwrap();
            let secret = group.secret_value().await?;
            let public = group.public_value(&secret)?;
            (secret, public)
        };

        // TODO: Check that the ServerName against our host name (or the host name or
        // our certificates).

        // When being requested with an ip address, we won't have a host name.
        if let Some(server_name) = find_server_name(&client_hello.extensions) {
            if server_name.names.len() != 1 {
                return Err(err_msg("Expected request to have exactly one name"));
            }

            // TODO: Check that certificate_auth has at least one cert.
            let name = std::str::from_utf8(&server_name.names[0].data)?;
            if server_name.names[0].typ != NameType::host_name
                || !self.options.certificate_auth.certificates[0].for_dns_name(name)?
            {
                return Err(format_err!(
                    "Our certificate is not valid for the requested domain: {}",
                    name
                ));
            }
        }

        // Find a KeyShareClientHello and use that to return a ServerHello

        let mut random = vec![0u8; 32];
        secure_random_bytes(&mut random).await?;

        let mut extensions = vec![];

        extensions.push(Extension::SupportedVersionsServerHello(
            SupportedVersionsServerHello {
                selected_version: TLS_1_3_VERSION,
            },
        ));

        extensions.push(Extension::KeyShareServerHello(KeyShareServerHello {
            server_share: KeyShareEntry {
                group: client_key_share.group,
                key_exchange: server_public.into(),
            },
        }));

        // TODO: Verify that this is supported by client and server.
        let cipher_suite = {
            let mut selected = None;
            for suite in &client_hello.cipher_suites {
                if !self.options.supported_cipher_suites.contains(suite) {
                    continue;
                }

                if let Ok(CipherSuiteParts::TLS13(_)) = suite.decode() {
                    selected = Some(*suite);
                }
            }

            selected.ok_or_else(|| err_msg("Can't agree on a cipher suite with the client"))?
        };

        let server_hello = ServerHello {
            legacy_version: TLS_1_2_VERSION,
            random: random.into(),
            legacy_session_id_echo: client_hello.legacy_session_id.clone(),
            cipher_suite: cipher_suite.clone(),
            legacy_compression_method: 0,
            extensions,
        };

        self.executor
            .send_handshake_message(Handshake::ServerHello(server_hello))
            .await?;

        // Create key schedule

        let key_schedule = KeyScheduleHelper::create_for_handshake(
            true,
            cipher_suite,
            client_key_share.group,
            &client_key_share.key_exchange,
            &server_secret,
            &self.executor.handshake_transcript,
            &mut self.executor.reader,
            &mut self.executor.writer,
        )?;

        let mut encrypted_extensions = vec![];

        // TODO: In TLS 1.2, this can only be sent in the ServerHello.
        if let Some(name_list) = find_alpn_extension(&client_hello.extensions) {
            for name in &name_list.names {
                if self.options.alpn_ids.contains(name) {
                    self.summary.selected_alpn_protocol = Some(name.clone());
                    encrypted_extensions.push(Extension::ALPN(ProtocolNameList {
                        names: vec![name.clone()],
                    }));
                    break;
                }
            }
        }

        self.executor
            .send_handshake_message(Handshake::EncryptedExtensions(EncryptedExtensions {
                extensions: encrypted_extensions,
            }))
            .await?;

        let cert_req_send = self.maybe_send_certificate_request(&client_hello).await?;

        self.executor
            .send_certificate(&self.options.certificate_auth, Bytes::new())
            .await?;

        let cert_ver = self
            .executor
            .create_certificate_verify(
                &key_schedule,
                &client_signature_algorithms_ext.algorithms,
                &self.options.certificate_auth.private_key,
            )
            .await?;

        self.executor
            .send_handshake_message(Handshake::CertificateVerify(cert_ver))
            .await?;

        let verify_data_server =
            key_schedule.verify_data_server(&self.executor.handshake_transcript);

        self.executor
            .send_handshake_message(Handshake::Finished(Finished {
                verify_data: verify_data_server,
            }))
            .await?;

        // TODO: Can also obtain the "resumption_master_secret" after we incorporate the
        // 'client Finished' message into the transcript.
        let server_finished_secrets =
            key_schedule.server_finished(&self.executor.handshake_transcript);

        if cert_req_send {
            let raw_certificate = match self.executor.receive_handshake_message().await? {
                Handshake::Certificate(c) => c,
                _ => {
                    return Err(err_msg("Expected certificate message"));
                }
            };

            let certificate_registry = self.certificate_registry.as_mut().unwrap();
            let options = self.options.certificate_request.as_ref().unwrap();

            let cert = self
                .executor
                .process_certificate(raw_certificate, certificate_registry, options, None)
                .await?;

            // NOTE: A client is allowed to advertise that they can do post_handshake_auth
            // and still send no certificates (and no CertificateVerify).
            if let Some(cert) = cert {
                self.executor
                    .receive_certificate_verify_v13(
                        &cert,
                        key_schedule.hasher_factory(),
                        certificate_registry,
                    )
                    .await?;

                self.summary.certificate = Some(cert);
            }
        }

        let verify_data_client =
            key_schedule.verify_data_client(&self.executor.handshake_transcript);
        self.executor.receive_finished(&verify_data_client).await?;

        self.executor
            .reader
            .replace_remote_key(server_finished_secrets.client_application_traffic_secret_0)?;

        self.executor
            .writer
            .replace_local_key(server_finished_secrets.server_application_traffic_secret_0)?;

        Ok(ApplicationStream::new(
            self.executor.reader,
            self.executor.writer,
            self.summary,
        ))
    }

    async fn maybe_send_certificate_request(&mut self, client_hello: &ClientHello) -> Result<bool> {
        if !has_post_handshake_auth(&client_hello.extensions)
            || self.options.certificate_request.is_none()
        {
            return Ok(false);
        }

        // TODO: Support other extensions like 'certificate_authorities' and
        // 'oid_filters'.
        let cert_req = CertificateRequest {
            certificate_request_context: Bytes::new(),
            // This is the only extension that MUST be present.
            extensions: vec![Extension::SignatureAlgorithms(SignatureSchemeList {
                algorithms: self.options.supported_signature_algorithms.clone(),
            })],
        };
        self.executor
            .send_handshake_message(Handshake::CertificateRequest(cert_req))
            .await?;

        Ok(true)
    }
}
