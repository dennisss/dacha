// This file includes the algorithms used for deriving the traffic keys for TLS
// 1.3. https://tools.ietf.org/html/rfc8446#section-7.1

use super::parsing::*;
use super::transcript::Transcript;
use crate::aead::*;
use crate::hasher::*;
use crate::hkdf::*;
use crate::utils::*;
use common::bytes::Bytes;

pub struct KeySchedule {
    hkdf: HKDF,
    hasher_factory: HasherFactory,

    current_salt: Vec<u8>,

    base_keys: Option<HandshakeTrafficSecrets>,

    // Useful precomputed values
    /// Vector of zeros of the same length as the hash function output.
    zero_salt: Vec<u8>,
    /// Hash of an empty transcript. aka hash("")
    empty_transcript_hash: Vec<u8>,
}

impl KeySchedule {
    pub fn new(hkdf: HKDF, hasher_factory: HasherFactory) -> Self {
        let zero_salt = vec![0u8; hkdf.hash_size()];
        let current_salt = zero_salt.clone();

        let empty_transcript_hash = {
            let mut hasher = hasher_factory.create();
            hasher.update(b"");
            hasher.finish()
        };

        Self {
            hkdf,
            hasher_factory,
            zero_salt,
            current_salt,
            empty_transcript_hash,
            base_keys: None,
        }
    }

    pub fn hasher_factory(&self) -> &HasherFactory {
        &self.hasher_factory
    }

    pub fn early_secret(&mut self, psk: Option<&[u8]>) -> &[u8] {
        let psk = self.zero_salt.clone();

        // Early Secret
        self.current_salt = self.hkdf.extract(&self.current_salt, &psk);
        &self.current_salt
    }

    pub fn handshake_secret(&mut self, shared_secret: &[u8]) -> &[u8] {
        // Derive-Secret(., "derived", "")
        self.current_salt = hkdf_expand_label(
            &self.hkdf,
            &self.current_salt,
            b"derived",
            &self.empty_transcript_hash,
            self.hkdf.hash_size() as u16,
        );

        // Handshake Secret
        self.current_salt = self.hkdf.extract(&self.current_salt, &shared_secret);
        &self.current_salt
    }

    /// Should be called immediately after a ServerHello
    pub fn handshake_traffic_secrets(
        &mut self,
        transcript: &Transcript,
    ) -> HandshakeTrafficSecrets {
        let ch_sh_transcript_hash = transcript.hash(&self.hasher_factory);

        let client_handshake_traffic_secret = hkdf_expand_label(
            &self.hkdf,
            &self.current_salt,
            b"c hs traffic",
            &ch_sh_transcript_hash,
            self.hkdf.hash_size() as u16,
        )
        .into();

        let server_handshake_traffic_secret = hkdf_expand_label(
            &self.hkdf,
            &self.current_salt,
            b"s hs traffic",
            &ch_sh_transcript_hash,
            self.hkdf.hash_size() as u16,
        )
        .into();

        self.base_keys = Some(HandshakeTrafficSecrets {
            client_handshake_traffic_secret,
            server_handshake_traffic_secret,
        });

        self.base_keys.clone().unwrap()
    }

    pub fn master_secret(&mut self) -> &[u8] {
        // Derive-Secret(., "derived", "")
        self.current_salt = hkdf_expand_label(
            &self.hkdf,
            &self.current_salt,
            b"derived",
            &self.empty_transcript_hash,
            self.hkdf.hash_size() as u16,
        );

        // Master Secret
        self.current_salt = self.hkdf.extract(&self.current_salt, &self.zero_salt);
        &self.current_salt
    }

    /// Call immediately before sending/receiving the server Finished message to
    /// calculate the corresponding verify_data.
    pub fn verify_data_server(&self, transcript: &Transcript) -> Bytes {
        let base_keys = self.base_keys.as_ref().unwrap();

        // finished_key =
        // 		HKDF-Expand-Label(BaseKey, "finished", "", Hash.length)
        let finished_key_server = hkdf_expand_label(
            &self.hkdf,
            &base_keys.server_handshake_traffic_secret,
            b"finished",
            b"",
            self.hkdf.hash_size() as u16,
        );

        let ch_cv_transcript_hash = transcript.hash(&self.hasher_factory);

        // verify_data =
        //  	HMAC(finished_key,
        //  		 Transcript-Hash(Handshake Context, Certificate*,
        // 							 CertificateVerify*))
        self.hkdf
            .extract(&finished_key_server, &ch_cv_transcript_hash)
            .into()
    }

    /// Call immediately after a server Finished message is sent/received to
    /// produce the expected client Finished verify_data.
    pub fn verify_data_client(&self, transcript: &Transcript) -> Bytes {
        let base_keys = self.base_keys.as_ref().unwrap();

        let finished_key_client = hkdf_expand_label(
            &self.hkdf,
            &base_keys.client_handshake_traffic_secret,
            b"finished",
            b"",
            self.hkdf.hash_size() as u16,
        );

        let ch_sf_transcript_hash = transcript.hash(&self.hasher_factory);

        self.hkdf
            .extract(&finished_key_client, &ch_sf_transcript_hash)
            .into()
    }

    /// NOTE: Should be called after the server Finished, but before any other
    /// messages are sent.
    pub fn finished(self, transcript: &Transcript) -> FinalSecrets {
        let ch_fin_transcript_hash = transcript.hash(&self.hasher_factory);

        FinalSecrets {
            client_application_traffic_secret_0: hkdf_expand_label(
                &self.hkdf,
                &self.current_salt,
                b"c ap traffic",
                &ch_fin_transcript_hash,
                self.hkdf.hash_size() as u16,
            )
            .into(),

            server_application_traffic_secret_0: hkdf_expand_label(
                &self.hkdf,
                &self.current_salt,
                b"s ap traffic",
                &ch_fin_transcript_hash,
                self.hkdf.hash_size() as u16,
            )
            .into(),

            exporter_master_secret: hkdf_expand_label(
                &self.hkdf,
                &self.current_salt,
                b"exp master",
                &ch_fin_transcript_hash,
                self.hkdf.hash_size() as u16,
            )
            .into(),

            resumption_master_secret: hkdf_expand_label(
                &self.hkdf,
                &self.current_salt,
                b"res master",
                &ch_fin_transcript_hash,
                self.hkdf.hash_size() as u16,
            )
            .into(),
        }
    }
}

#[derive(Clone)]
pub struct HandshakeTrafficSecrets {
    pub client_handshake_traffic_secret: Bytes,
    pub server_handshake_traffic_secret: Bytes,
}

pub struct FinalSecrets {
    pub client_application_traffic_secret_0: Bytes,
    pub server_application_traffic_secret_0: Bytes,
    pub exporter_master_secret: Bytes,
    pub resumption_master_secret: Bytes,
}

pub struct TrafficKey {
    pub key: Bytes,
    pub iv: Bytes,
}

/// Keying material derived from the traffic secret and is used to derive the
/// actual traffic keys
pub struct TrafficKeyingMaterial {
    base_key: TrafficKey,
    sequence: u64,
}

impl TrafficKeyingMaterial {
    // [sender]_write_key = HKDF-Expand-Label(Secret, "key", "", key_length)
    // [sender]_write_iv  = HKDF-Expand-Label(Secret, "iv", "", iv_length)
    pub fn from_secret(hkdf: &HKDF, aead: &dyn AuthEncAD, traffic_secret: &[u8]) -> Self {
        let key_length = aead.key_size();

        let iv_length = {
            let range = aead.nonce_range();
            assert!(range.1 >= 8);
            std::cmp::max(8, range.0)
        };

        let key = hkdf_expand_label(hkdf, traffic_secret, b"key", b"", key_length as u16).into();
        let iv = hkdf_expand_label(hkdf, traffic_secret, b"iv", b"", iv_length as u16).into();

        Self {
            base_key: TrafficKey { key, iv },
            sequence: 0,
        }
    }

    /// Calculates the keys for the next record.
    /// The returned key has a new never before used per-record nonce as
    /// described in:
    /// https://tools.ietf.org/html/rfc8446#section-5.3
    pub fn next_keys(&mut self) -> TrafficKey {
        let mut nonce = vec![0u8; self.base_key.iv.len()];
        *array_mut_ref![nonce, nonce.len() - 8, 8] = self.sequence.to_be_bytes();

        xor_inplace(&self.base_key.iv, &mut nonce);

        self.sequence += 1;

        TrafficKey {
            key: self.base_key.key.clone(),
            iv: nonce.into(),
        }
    }
}

// HKDF-Expand-Label(Secret, Label, Context, Length) =
// 	HKDF-Expand(Secret, HkdfLabel, Length)

pub fn hkdf_expand_label(
    hkdf: &HKDF,
    secret: &[u8],
    label: &[u8],
    context: &[u8],
    length: u16,
) -> Vec<u8> {
    let mut hdkf_label = vec![];
    HkdfLabel {
        length,
        label,
        context,
    }
    .serialize(&mut hdkf_label);

    hkdf.expand(secret, &hdkf_label, length as usize)
}

// Where HkdfLabel is specified as:
/*
struct {
    uint16 length = Length;
    opaque label<7..255> = "tls13 " + Label;
    opaque context<0..255> = Context;
} HkdfLabel;
*/
/// NOTE: This never needs to be parsed, so this has been optimized for
/// serialization.
struct HkdfLabel<'a> {
    length: u16,
    // NOTE: Don't include the 'tls13 ' prefix in this
    label: &'a [u8],
    context: &'a [u8],
}

impl HkdfLabel<'_> {
    // Serializes to a maximum size of 512.
    fn serialize(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&self.length.to_be_bytes());
        serialize_varlen_vector(7, 255, out, |out| {
            out.extend_from_slice(b"tls13 ");
            out.extend_from_slice(self.label);
        });
        serialize_varlen_vector(0, 255, out, |out| {
            out.extend_from_slice(self.context);
        });
    }
}

// Derive-Secret(Secret, Label, Messages) =
// 	HKDF-Expand-Label(Secret, Label,
// 						Transcript-Hash(Messages), Hash.length)
