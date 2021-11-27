use common::bytes::Bytes;
use common::errors::*;

use crate::aead::AuthEncAD;
use crate::tls::record::Record;

/// NOTE: A single instance of this should only be used for encrypting or
/// decrypting, but not both.
pub struct CipherEndpointSpecTLS12 {
    /// Sequence number for the next record to be sent with this cipher.
    /// Starts at 0 for the first record sent with this cipher. Never wraps.
    sequence_num: u64,

    mac_key: Bytes,

    encryption_key: Bytes,

    implicit_iv: Bytes,

    aead: Box<dyn AuthEncAD>,

    nonce_gen: Box<dyn NonceGenerator>,
}

// pub enum CipherEndpointSpecTypeTLS12 {
//     AEAD(Box<dyn AuthEncAD>)
// }

impl CipherEndpointSpecTLS12 {
    pub fn new(
        mac_key: Bytes,
        encryption_key: Bytes,
        implicit_iv: Bytes,
        aead: Box<dyn AuthEncAD>,
        nonce_gen: Box<dyn NonceGenerator>,
    ) -> Self {
        Self {
            sequence_num: 0,
            mac_key,
            encryption_key,
            implicit_iv,
            aead,
            nonce_gen,
        }
    }

    /// Encrypts a TLS 1.2 TLSCompressed (or TLSPlaintext if no compression is
    /// used) record into a TLSCiphertext record used this cipher.
    pub fn encrypt(&mut self, record: Record) -> Record {
        let mut additional_data = vec![];
        additional_data.extend_from_slice(&self.sequence_num.to_be_bytes());
        additional_data.push(record.typ.to_u8());
        additional_data.extend_from_slice(&record.legacy_record_version.to_be_bytes());
        additional_data.extend_from_slice(&(record.data.len() as u16).to_be_bytes());

        // Creating the GenericAEADCipher struct.
        let mut data = vec![];

        let explicit_nonce = self.nonce_gen.generate_explicit(self);
        data.extend_from_slice(&explicit_nonce);

        let key = &self.encryption_key;
        let nonce = self.nonce_gen.generate_full(self, &explicit_nonce);

        // TODO: Directly outputing the cipher text to the 'data' array is currently
        // broken.
        {
            let mut cipher = vec![];
            self.aead
                .encrypt(key, &nonce, &record.data, &additional_data, &mut cipher);

            data.extend_from_slice(&cipher);
        }

        self.sequence_num += 1;

        Record {
            legacy_record_version: record.legacy_record_version,
            typ: record.typ,
            data: data.into(),
        }
    }

    pub fn decrypt(&mut self, record: Record) -> Result<Record> {
        let explicit_nonce_size = self.nonce_gen.explicit_size();
        if record.data.len() < explicit_nonce_size {
            return Err(err_msg("Missing explicit nonce"));
        }

        let (explicit_nonce, ciphertext) = record.data.split_at(explicit_nonce_size);

        // TODO: Dedup this code.
        let mut additional_data = vec![];
        additional_data.extend_from_slice(&self.sequence_num.to_be_bytes());
        additional_data.push(record.typ.to_u8());
        additional_data.extend_from_slice(&record.legacy_record_version.to_be_bytes());

        let plaintext_len = record.data.len() - self.aead.expanded_size(0) - explicit_nonce_size;

        // MUST exclude MAC tag and nonce
        additional_data.extend_from_slice(&(plaintext_len as u16).to_be_bytes());

        let key = &self.encryption_key[..];
        let nonce = self.nonce_gen.generate_full(self, explicit_nonce);

        let mut plaintext = vec![];
        self.aead
            .decrypt(key, &nonce, ciphertext, &additional_data, &mut plaintext)?;

        self.sequence_num += 1;

        Ok(Record {
            legacy_record_version: record.legacy_record_version,
            typ: record.typ,
            data: plaintext.into(),
        })
    }
}

pub trait NonceGenerator: 'static + Send + Sync {
    fn explicit_size(&self) -> usize;

    /// Should return the value of 'fixed_iv_length' for this cipher.
    fn implicit_size(&self) -> usize;

    /// Generates the explicit nonce which is sent in each TLS 1.2 packet.
    ///
    /// This is the value of length 'record_iv_length' mentioned in the TLS 1.2
    /// spec.
    fn generate_explicit(&self, cipher_spec: &CipherEndpointSpecTLS12) -> Vec<u8>;

    fn generate_full(&self, cipher_spec: &CipherEndpointSpecTLS12, explicit: &[u8]) -> Vec<u8>;

    fn box_clone(&self) -> Box<dyn NonceGenerator>;
}

/*
AES GCM:
- Always 128-bit GCM
    - Ignore any truncated_mac extensions in the extensions
- Nonce is 12 bytes:
    struct {
        opaque salt[4]; - "implicit" part of the nonce and NOT sent in the packaet
            client_write_IV / server* (4 bytes)  (fixed_iv_length)
        opaque nonce_explicit[8];
            record_iv_length
            GenericAEADCipher.nonce_explicit - sent in the packet
            - Can just be the 64-bit sequence number.
    } GCMNonce;

    - No mac key

*/

/// Nonce generator for AES GCM AEAD ciphers.
/// Based on RFC 5288 (https://datatracker.ietf.org/doc/html/rfc5288)
#[derive(Clone)]
pub struct GCMNonceGenerator {}

impl GCMNonceGenerator {
    pub fn new() -> Self {
        Self {}
    }
}

impl NonceGenerator for GCMNonceGenerator {
    fn explicit_size(&self) -> usize {
        8
    }

    fn implicit_size(&self) -> usize {
        4
    }

    fn generate_explicit(&self, cipher_spec: &CipherEndpointSpecTLS12) -> Vec<u8> {
        // Always 8 bytes. Just use the sequence number
        cipher_spec.sequence_num.to_be_bytes().to_vec()
    }

    fn generate_full(&self, cipher_spec: &CipherEndpointSpecTLS12, explicit: &[u8]) -> Vec<u8> {
        // Concatenate the implicit 'salt' with the explicit nonce.
        let mut out = cipher_spec.implicit_iv.to_vec();
        out.extend_from_slice(explicit);
        out
    }

    fn box_clone(&self) -> Box<dyn NonceGenerator> {
        Box::new(self.clone())
    }
}

/// Based on RFC 7905 (https://datatracker.ietf.org/doc/html/rfc7905)
#[derive(Clone)]
pub struct ChaChaPoly1305NonceGenerator {}

impl ChaChaPoly1305NonceGenerator {
    pub fn new() -> Self {
        Self {}
    }
}

impl NonceGenerator for ChaChaPoly1305NonceGenerator {
    fn explicit_size(&self) -> usize {
        0
    }

    fn implicit_size(&self) -> usize {
        12
    }

    fn generate_explicit(&self, cipher_spec: &CipherEndpointSpecTLS12) -> Vec<u8> {
        vec![]
    }

    fn generate_full(&self, cipher_spec: &CipherEndpointSpecTLS12, explicit: &[u8]) -> Vec<u8> {
        let mut out = vec![0u8; 12];
        out[4..].copy_from_slice(&cipher_spec.sequence_num.to_be_bytes());
        crate::utils::xor_inplace(&cipher_spec.implicit_iv, &mut out);
        out
    }

    fn box_clone(&self) -> Box<dyn NonceGenerator> {
        Box::new(self.clone())
    }
}
