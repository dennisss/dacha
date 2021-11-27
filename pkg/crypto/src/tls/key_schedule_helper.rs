use common::errors::*;

use crate::hkdf::HKDF;
use crate::tls::cipher::*;
use crate::tls::cipher_suite::{CipherSuite, CipherSuiteParts};
use crate::tls::extensions::NamedGroup;
use crate::tls::key_schedule::*;
use crate::tls::record_stream::{RecordReader, RecordWriter};
use crate::tls::transcript::Transcript;

pub struct KeyScheduleHelper {}

impl KeyScheduleHelper {
    /// Once the server and client have agreed on a cipher and DHE group, this
    /// uses the shared values to create the initial key schedule and initialize
    /// the handshake traffic keys.
    ///
    /// Arguments:
    /// - is_server:
    /// - cipher_suite:
    /// - group:
    /// - remote_public:
    /// - local_secret:
    /// - handshake_transcript: Should contain the ClientHello and ServerHello.
    /// - reader:
    /// - writer:
    pub fn create_for_handshake(
        is_server: bool,
        cipher_suite: CipherSuite,
        group: NamedGroup,
        remote_public: &[u8],
        local_secret: &[u8],
        handshake_transcript: &Transcript,
        reader: &mut RecordReader,
        writer: &mut RecordWriter,
    ) -> Result<KeySchedule> {
        let (aead, hasher_factory) = match cipher_suite.decode()? {
            CipherSuiteParts::TLS13(suite) => (suite.aead, suite.hasher_factory),
            _ => {
                return Err(err_msg("Bad cipher suite"));
            }
        };

        let hkdf = HKDF::new(hasher_factory.box_clone());

        let mut key_schedule = KeySchedule::new(hkdf.clone(), hasher_factory.box_clone());

        // TODO: Use the early secret.
        key_schedule.early_secret(None);

        // Given that the caller was able to create a local secret for this group, it
        // should always be supported.
        let group = group.create().unwrap();

        let shared_secret = group.shared_secret(remote_public, local_secret)?;

        // NOTE: The return value of this isn't used directly and will instead by used
        // below to create the handshake traffic keys.
        key_schedule.handshake_secret(&shared_secret);

        let (client_handshake_traffic_secret, server_handshake_traffic_secret) = {
            let s = key_schedule.handshake_traffic_secrets(handshake_transcript);
            (
                s.client_handshake_traffic_secret,
                s.server_handshake_traffic_secret,
            )
        };

        // TODO: Don't do this until we need the application secrets?
        key_schedule.master_secret();

        let (local_traffic_secret, remote_traffic_secret) = {
            if is_server {
                (
                    server_handshake_traffic_secret,
                    client_handshake_traffic_secret,
                )
            } else {
                (
                    client_handshake_traffic_secret,
                    server_handshake_traffic_secret,
                )
            }
        };

        writer.local_cipher_spec = Some(CipherEndpointSpec::TLS13(CipherEndpointSpecTLS13::new(
            aead.box_clone(),
            hkdf.clone(),
            local_traffic_secret,
        )));

        reader.set_remote_cipher_spec(CipherEndpointSpec::TLS13(CipherEndpointSpecTLS13::new(
            aead.box_clone(),
            hkdf.clone(),
            remote_traffic_secret,
        )))?;

        Ok(key_schedule)
    }
}
