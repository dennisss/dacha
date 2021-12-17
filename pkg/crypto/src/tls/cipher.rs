use alloc::boxed::Box;
use common::async_std::sync::Mutex;
use common::bytes::Bytes;
use common::errors::*;

use crate::aead::AuthEncAD;
use crate::hkdf::HKDF;
use crate::tls::cipher_tls12::CipherEndpointSpecTLS12;
use crate::tls::key_schedule::*;
use crate::tls::record::{ContentType, Record};

pub enum CipherEndpointSpec {
    TLS12(CipherEndpointSpecTLS12),
    TLS13(CipherEndpointSpecTLS13),
}

/// Defines how to encrypt/decrypt data on one half of a TLS 1.3 connection.
///
/// This is negotiated during the TLS handshake and defines which algorithm to
/// use for encryption, what keys are currently in play, and how the keys will
/// change in the future.
///
/// While this only defines one half of the keys in the connection, the other
/// side will almost always be using the same AEAD and HKDF config.
pub struct CipherEndpointSpecTLS13 {
    aead: Box<dyn AuthEncAD>,

    hkdf: HKDF,

    traffic_secret: Bytes,

    /// Derived from the above traffic secret.
    keying: TrafficKeyingMaterial,
}

impl CipherEndpointSpecTLS13 {
    pub fn new(aead: Box<dyn AuthEncAD>, hkdf: HKDF, traffic_secret: Bytes) -> Self {
        let keying = TrafficKeyingMaterial::from_secret(&hkdf, aead.as_ref(), &traffic_secret);
        Self {
            aead,
            hkdf,
            traffic_secret,
            keying,
        }
    }

    pub fn replace_key(&mut self, traffic_secret: Bytes) {
        self.keying =
            TrafficKeyingMaterial::from_secret(&self.hkdf, self.aead.as_ref(), &traffic_secret);
        self.traffic_secret = traffic_secret;
    }

    /// Switches to using the next application secret / keys. This should
    /// correspond to sending / receiving a KeyUpdate.
    ///
    /// NOTE: It's only valid to call this after the TLS handshake.
    ///
    /// application_traffic_secret_N+1 =
    ///        HKDF-Expand-Label(application_traffic_secret_N,
    ///                          "traffic upd", "", Hash.length)
    pub fn update_key(&mut self) {
        let next_secret = hkdf_expand_label(
            &self.hkdf,
            &self.traffic_secret,
            b"traffic upd",
            b"",
            self.hkdf.hash_size() as u16,
        )
        .into();

        self.replace_key(next_secret);
    }

    pub fn encrypt(&mut self, record: Record) -> Record {
        let typ = ContentType::ApplicationData;

        // How much padding to add to each plaintext record.
        // TODO: Support padding up to a block size or accepting a callback
        // to configure this.
        let padding_size = 0;

        // Total expected size of cipher text. We need one byte at the end
        // for the content type.
        let total_size = self.aead.expanded_size(record.data.len() + 1) + padding_size;

        let mut additional_data = vec![];
        typ.serialize(&mut additional_data);
        additional_data.extend_from_slice(&record.legacy_record_version.to_be_bytes());
        additional_data.extend_from_slice(&(total_size as u16).to_be_bytes());

        // Serialize the record inner.
        let mut plaintext = vec![];
        plaintext.resize(record.data.len() + 1 + padding_size, 0);
        plaintext[0..record.data.len()].copy_from_slice(&record.data);
        plaintext[record.data.len()] = record.typ.to_u8();

        let key = self.keying.next_keys();

        let mut ciphertext = vec![];
        ciphertext.reserve(total_size);
        self.aead.encrypt(
            &key.key,
            &key.iv,
            &plaintext,
            &additional_data,
            &mut ciphertext,
        );

        assert_eq!(ciphertext.len(), total_size);

        Record {
            legacy_record_version: record.legacy_record_version,
            typ,
            data: ciphertext.into(),
        }
    }

    pub fn decrypt(&mut self, record: Record) -> Result<Record> {
        if record.typ != ContentType::ApplicationData {
            return Err(err_msg("Expected only encrypted data not"));
        }

        let key = self.keying.next_keys();

        // additional_data = TLSCiphertext.opaque_type ||
        //     TLSCiphertext.legacy_record_version ||
        //     TLSCiphertext.length
        // TODO: Implement this as a slice of the original record.
        let mut additional_data = vec![];
        record.typ.serialize(&mut additional_data);
        additional_data.extend_from_slice(&record.legacy_record_version.to_be_bytes());
        additional_data.extend_from_slice(&(record.data.len() as u16).to_be_bytes());

        let mut plaintext = vec![];
        self.aead.decrypt(
            &key.key,
            &key.iv,
            &record.data,
            &additional_data,
            &mut plaintext,
        )?;

        // TODO: Move to REcordInner struct
        // The content type is the the last non-zero byte. All zeros
        // after that are padding and can be ignored.
        let mut content_type_res = None;
        for i in (0..plaintext.len()).rev() {
            if plaintext[i] != 0 {
                content_type_res = Some(i);
                break;
            }
        }

        let content_type_i = content_type_res.ok_or_else(|| err_msg("All zero"))?;

        let content_type = ContentType::from_u8(plaintext[content_type_i]);

        plaintext.truncate(content_type_i);

        Ok(Record {
            legacy_record_version: record.legacy_record_version,
            typ: content_type,
            data: plaintext.into(),
        })
    }
}
