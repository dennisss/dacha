use asn::encoding::DERWriteable;
use common::async_std::net::TcpListener;
use common::bytes::Bytes;
use common::errors::*;
use common::futures::StreamExt;
use common::io::{Readable, Writeable};
use pkix::PKIX1Algorithms2008;

use crate::hasher::GetHasherFactory;
use crate::random::secure_random_bytes;
use crate::tls::application_stream::ApplicationStream;
use crate::tls::cipher_suite::*;
use crate::tls::constants::*;
use crate::tls::extensions::*;
use crate::tls::extensions_util::*;
use crate::tls::handshake::{
    Certificate, CertificateEntry, CertificateVerify, EncryptedExtensions, Finished, Handshake,
    ServerHello, TLS_1_2_VERSION, TLS_1_3_VERSION,
};
use crate::tls::handshake_summary::HandshakeSummary;
use crate::tls::key_schedule::KeySchedule;
use crate::tls::key_schedule_helper::KeyScheduleHelper;
use crate::tls::options::ServerOptions;
use crate::tls::record_stream::Message;
use crate::tls::record_stream::{RecordReader, RecordWriter};
use crate::tls::transcript::Transcript;
use crate::x509;

use super::handshake_executor::HandshakeExecutor;

/*
- Load the Certificate and private key files into memory.

- Wait for ClientHello

- Figure out if using TLS 1.3
    - Must minally have a supported_versions extension with TLS 1.3 mentioned.

- Either send a ServerHello or Retry record


- Send Certificate

- Send CertificateVerify



*/

// TODO: We want a ServerHandshakeExecutor struct to separate out the logic.

pub struct Server {}

impl Server {
    pub async fn run(port: u16, options: &ServerOptions) -> Result<()> {
        // Bind all all interfaces.
        let listener = TcpListener::bind(format!("0.0.0.0:{}", port)).await?;

        let mut incoming = listener.incoming();
        while let Some(stream) = incoming.next().await {
            let stream = stream?;

            let mut app =
                Self::connect(Box::new(stream.clone()), Box::new(stream), options).await?;
            app.writer.write_all(b"Hello world").await?;
        }

        Ok(())
    }

    pub async fn connect(
        reader: Box<dyn Readable + Sync>,
        writer: Box<dyn Writeable>,
        options: &ServerOptions,
    ) -> Result<ApplicationStream> {
        ServerHandshakeExecutor::new(reader, writer, options)
            .run()
            .await
    }
}

struct ServerHandshakeExecutor<'a> {
    executor: HandshakeExecutor,
    options: &'a ServerOptions,
    summary: HandshakeSummary,
}

impl<'a> ServerHandshakeExecutor<'a> {
    pub fn new(
        reader: Box<dyn Readable + Sync>,
        writer: Box<dyn Writeable>,
        options: &'a ServerOptions,
    ) -> Self {
        Self {
            executor: HandshakeExecutor::new(
                RecordReader::new(reader, true),
                RecordWriter::new(writer, true),
            ),
            options,
            summary: HandshakeSummary::default(),
        }
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

        let server_name = find_server_name(&client_hello.extensions)
            .ok_or_else(|| err_msg("Expected request to have a server name extension"))?;
        if server_name.names.len() != 1 {
            return Err(err_msg("Expected request to have exactly one name"));
        }

        let name = std::str::from_utf8(&server_name.names[0].data)?;
        if server_name.names[0].typ != NameType::host_name
            || !self.options.certificates[0].for_dns_name(name)?
        {
            return Err(format_err!(
                "Our certificate is not valid for the requested domain: {}",
                name
            ));
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

        if let Some(name_list) = find_alpn_extension(&client_hello.extensions) {
            for name in &name_list.names {
                if self.options.alpn_ids.contains(name) {
                    self.summary.selected_alpn_protocol = Some(name.clone());
                    extensions.push(Extension::ALPN(ProtocolNameList {
                        names: vec![name.clone()],
                    }));
                    break;
                }
            }
        }

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

        // CipherSuite::TLS_CHACHA20_POLY1305_SHA256;

        let server_hello = ServerHello {
            legacy_version: TLS_1_2_VERSION,
            random: random.into(),
            legacy_session_id_echo: client_hello.legacy_session_id,
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

        self.executor
            .send_handshake_message(Handshake::EncryptedExtensions(EncryptedExtensions {
                extensions: vec![],
            }))
            .await?;

        // CertificateRequest
        // - Can only send if client has used PostHandshakeAuth
        // - with certificate_authorities
        // - oid_filters
        // - MUST have signature_algorithms
        // - If a client declines, it will send a Certificate that has an empty list and
        //   not CertificateVerify

        let mut certificate_list = vec![];
        for cert in &self.options.certificates {
            certificate_list.push(CertificateEntry {
                cert: cert.raw.to_der().into(), /* TODO: Can we implement this without
                                                 * re-serialization. */
                extensions: vec![],
            });
        }

        let certs = Handshake::Certificate(Certificate {
            certificate_request_context: Bytes::new(),
            certificate_list,
        });

        self.executor.send_handshake_message(certs).await?;

        let cert_ver = self
            .executor
            .create_certificate_verify(
                true,
                &key_schedule,
                &self.options.supported_signature_algorithms,
                &client_signature_algorithms_ext.algorithms,
                &self.options.private_key,
            )
            .await?;

        self.executor
            .send_handshake_message(Handshake::CertificateVerify(cert_ver))
            .await?;

        self.wait_finished(key_schedule).await?;

        Ok(ApplicationStream::new(
            self.executor.reader,
            self.executor.writer,
            self.summary,
        ))
    }

    async fn wait_finished(&mut self, key_schedule: KeySchedule) -> Result<()> {
        let verify_data_server =
            key_schedule.verify_data_server(&self.executor.handshake_transcript);

        self.executor
            .send_handshake_message(Handshake::Finished(Finished {
                verify_data: verify_data_server,
            }))
            .await?;

        let verify_data_client =
            key_schedule.verify_data_client(&self.executor.handshake_transcript);

        // TODO: Can also obtain the "resumption_master_secret" after we incorporate the
        // 'client Finished' message into the transcript.
        let final_secrets = key_schedule.finished(&self.executor.handshake_transcript);

        let finished_client = match self.executor.receive_handshake_message().await? {
            Handshake::Finished(v) => v,
            _ => {
                return Err(err_msg("Expected client finished"));
            }
        };

        if finished_client.verify_data != verify_data_client {
            return Err(err_msg("Incorrect client verify_data"));
        }

        self.executor
            .reader
            .replace_remote_key(final_secrets.client_application_traffic_secret_0)?;

        self.executor
            .writer
            .replace_local_key(final_secrets.server_application_traffic_secret_0)?;

        Ok(())
    }
}
