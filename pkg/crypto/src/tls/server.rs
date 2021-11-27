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
        reader: Box<dyn Readable>,
        writer: Box<dyn Writeable>,
        options: &ServerOptions,
    ) -> Result<ApplicationStream> {
        ServerHandshakeExecutor::new(reader, writer, options)
            .run()
            .await
    }
}

struct ServerHandshakeExecutor<'a> {
    reader: RecordReader,
    writer: RecordWriter,
    options: &'a ServerOptions,

    handshake_transcript: Transcript,

    summary: HandshakeSummary,
}

impl<'a> ServerHandshakeExecutor<'a> {
    pub fn new(
        reader: Box<dyn Readable>,
        writer: Box<dyn Writeable>,
        options: &'a ServerOptions,
    ) -> Self {
        Self {
            reader: RecordReader::new(reader, true),
            writer: RecordWriter::new(writer, true),
            options,
            handshake_transcript: Transcript::new(),
            summary: HandshakeSummary::default(),
        }
    }

    /// TODO: Pick a better name for this?
    /// TODO: Filter out io errors.
    pub async fn run(mut self) -> Result<ApplicationStream> {
        let message = self
            .reader
            .recv(Some(&mut self.handshake_transcript))
            .await?;

        let handshake = match message {
            Message::ChangeCipherSpec(_) => todo!(),
            Message::Alert(_) => todo!(),
            Message::Handshake(v) => v,
            Message::ApplicationData(_) => todo!(),
        };

        let client_hello = match handshake {
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

        // TODO: Also append ALPN selection.

        // TODO: Verify that this is supported by client and server.
        let cipher_suite = CipherSuite::TLS_CHACHA20_POLY1305_SHA256;

        let server_hello = ServerHello {
            legacy_version: TLS_1_2_VERSION,
            random: random.into(),
            legacy_session_id_echo: client_hello.legacy_session_id,
            cipher_suite: cipher_suite.clone(),
            legacy_compression_method: 0,
            extensions,
        };

        self.writer
            .send_handshake(
                Handshake::ServerHello(server_hello),
                Some(&mut self.handshake_transcript),
            )
            .await?;

        // Create key schedule

        let key_schedule = KeyScheduleHelper::create_for_handshake(
            true,
            cipher_suite,
            client_key_share.group,
            &client_key_share.key_exchange,
            &server_secret,
            &self.handshake_transcript,
            &mut self.reader,
            &mut self.writer,
        )?;

        self.writer
            .send_handshake(
                Handshake::EncryptedExtensions(EncryptedExtensions { extensions: vec![] }),
                Some(&mut self.handshake_transcript),
            )
            .await?;

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

        self.writer
            .send_handshake(certs, Some(&mut self.handshake_transcript))
            .await?;

        let cert_ver = self
            .create_certificate_verify(&key_schedule, &client_signature_algorithms_ext.algorithms)
            .await?;
        self.writer
            .send_handshake(
                Handshake::CertificateVerify(cert_ver),
                Some(&mut self.handshake_transcript),
            )
            .await?;

        self.wait_finished(key_schedule).await?;

        Ok(ApplicationStream::new(
            self.reader,
            self.writer,
            self.summary,
        ))
    }

    // TODO: Deduplicate much of this logic with the client.
    async fn create_certificate_verify(
        &self,
        key_schedule: &KeySchedule,
        client_supported_algorithms: &[SignatureScheme],
    ) -> Result<CertificateVerify> {
        // Transcript hash for ClientHello through to the Certificate.
        let ch_ct_transcript_hash = self
            .handshake_transcript
            .hash(key_schedule.hasher_factory());

        let mut plaintext = vec![];
        for _ in 0..64 {
            plaintext.push(0x20);
        }
        plaintext.extend_from_slice(&TLS13_CERTIFICATEVERIFY_SERVER_CTX);
        plaintext.push(0);
        plaintext.extend_from_slice(&ch_ct_transcript_hash);

        // Select the best signature scheme.
        // - Must be supported by the client.
        // - Must be supported by our certificate/private key.
        let mut selected_signature_algorithm = None;
        for algorithm in &self.options.supported_signature_algorithms {
            if !client_supported_algorithms.contains(algorithm) {
                continue;
            }

            let compatible = match &self.options.private_key {
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
                let (_, group, private_key) = match &self.options.private_key {
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
                let private_key = match &self.options.private_key {
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

                let private_key = match &self.options.private_key {
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

    async fn wait_finished(&mut self, key_schedule: KeySchedule) -> Result<()> {
        let verify_data_server = key_schedule.verify_data_server(&self.handshake_transcript);

        self.writer
            .send_handshake(
                Handshake::Finished(Finished {
                    verify_data: verify_data_server,
                }),
                Some(&mut self.handshake_transcript),
            )
            .await?;

        let verify_data_client = key_schedule.verify_data_client(&self.handshake_transcript);

        // TODO: Can also obtain the "resumption_master_secret" after we incorporate the
        // 'client Finished' message into the transcript.
        let final_secrets = key_schedule.finished(&self.handshake_transcript);

        let finished_client = match self
            .reader
            .recv(Some(&mut self.handshake_transcript))
            .await?
        {
            Message::Handshake(Handshake::Finished(v)) => v,
            _ => {
                return Err(err_msg("Expected client finished"));
            }
        };

        if finished_client.verify_data != verify_data_client {
            return Err(err_msg("Incorrect client verify_data"));
        }

        self.reader
            .replace_remote_key(final_secrets.client_application_traffic_secret_0)?;

        self.writer
            .replace_local_key(final_secrets.server_application_traffic_secret_0)?;

        Ok(())
    }
}
