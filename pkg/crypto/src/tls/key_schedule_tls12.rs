// Helpers for deriving the traffic keys used in TLS 1.2.

use common::bytes::Bytes;
use common::errors::*;

use crate::hasher::HasherFactory;
use crate::hmac::HMAC;
use crate::tls::cipher::CipherEndpointSpec;
use crate::tls::cipher_suite::CipherSuiteTLS12;
use crate::tls::cipher_tls12::*;
use crate::tls::handshake::{ClientHello, ServerHello};

use super::transcript::Transcript;

// TODO: Parameterize this based on the CipherSuite.
const VERIFY_DATA_LENGTH: usize = 12;

pub struct KeyScheduleTLS12 {
    cipher_suite: CipherSuiteTLS12,
    master_secret: Vec<u8>,
    key_block: KeyBlock,
}

impl KeyScheduleTLS12 {
    pub fn create(
        cipher_suite: CipherSuiteTLS12,
        pre_master_secret: &[u8],
        client_hello: &ClientHello,
        server_hello: &ServerHello,
    ) -> Self {
        let master_secret = master_secret(
            pre_master_secret,
            client_hello,
            server_hello,
            &cipher_suite.hasher_factory,
        );

        let key_block = key_block(
            &master_secret,
            client_hello,
            server_hello,
            &cipher_suite.hasher_factory,
            0, // Valid for all AEAD ciphers.
            cipher_suite.aead.key_size(),
            cipher_suite.nonce_gen.implicit_size(),
        );

        Self {
            cipher_suite,
            master_secret,
            key_block,
        }
    }

    pub fn client_cipher_spec(&self) -> CipherEndpointSpec {
        // TODO: Shouldn't need to clone the key references.
        CipherEndpointSpec::TLS12(CipherEndpointSpecTLS12::new(
            self.key_block.client_write_mac_key.clone(),
            self.key_block.client_write_key.clone(),
            self.key_block.client_write_iv.clone(),
            self.cipher_suite.aead.box_clone(),
            self.cipher_suite.nonce_gen.box_clone(),
        ))
    }

    pub fn server_cipher_spec(&self) -> CipherEndpointSpec {
        CipherEndpointSpec::TLS12(CipherEndpointSpecTLS12::new(
            self.key_block.server_write_mac_key.clone(),
            self.key_block.server_write_key.clone(),
            self.key_block.server_write_iv.clone(),
            self.cipher_suite.aead.box_clone(),
            self.cipher_suite.nonce_gen.box_clone(),
        ))
    }

    /// Transcript should include all handshake messages from the first
    /// ClientHello up to and including the last handshake message before
    /// sending the client Finished record (usually the ClientKeyExchange).
    pub fn verify_data_client(&self, handshake_transcript: &Transcript) -> Vec<u8> {
        let hash = handshake_transcript.hash(&self.cipher_suite.hasher_factory);
        prf(
            &self.master_secret,
            b"client finished",
            &hash,
            VERIFY_DATA_LENGTH,
            &self.cipher_suite.hasher_factory,
        )
    }

    /// Transcript should usually end with the Finished message from the client.
    pub fn verify_data_server(&self, handshake_transcript: &Transcript) -> Vec<u8> {
        let hash = handshake_transcript.hash(&self.cipher_suite.hasher_factory);
        prf(
            &self.master_secret,
            b"server finished",
            &hash,
            VERIFY_DATA_LENGTH,
            &self.cipher_suite.hasher_factory,
        )
    }
}

#[derive(Debug)]
struct KeyBlock {
    client_write_mac_key: Bytes,
    server_write_mac_key: Bytes,
    client_write_key: Bytes,
    server_write_key: Bytes,
    client_write_iv: Bytes,
    server_write_iv: Bytes,
}

/// key_block = PRF(SecurityParameters.master_secret,
///     "key expansion",
///     SecurityParameters.server_random +
///     SecurityParameters.client_random);
fn key_block(
    master_secret: &[u8],
    client_hello: &ClientHello,
    server_hello: &ServerHello,
    hasher_factory: &HasherFactory,
    mac_key_length: usize,
    enc_key_length: usize,
    fixed_iv_length: usize,
) -> KeyBlock {
    let block_size = 2 * (mac_key_length + enc_key_length + fixed_iv_length);

    let mut seed = vec![];
    seed.extend_from_slice(&server_hello.random);
    seed.extend_from_slice(&client_hello.random);

    let mut block = Bytes::from(prf(
        master_secret,
        b"key expansion",
        &seed,
        block_size,
        hasher_factory,
    ));

    let client_write_mac_key = block.split_to(mac_key_length);
    let server_write_mac_key = block.split_to(mac_key_length);

    let client_write_key = block.split_to(enc_key_length);
    let server_write_key = block.split_to(enc_key_length);

    let client_write_iv = block.split_to(fixed_iv_length);
    let server_write_iv = block.split_to(fixed_iv_length);

    assert_eq!(block.len(), 0);

    KeyBlock {
        client_write_mac_key,
        server_write_mac_key,
        client_write_key,
        server_write_key,
        client_write_iv,
        server_write_iv,
    }
}

/// master_secret = PRF(pre_master_secret, "master secret",
///     ClientHello.random + ServerHello.random)
///     [0..47];
fn master_secret(
    pre_master_secret: &[u8],
    client_hello: &ClientHello,
    server_hello: &ServerHello,
    hasher_factory: &HasherFactory,
) -> Vec<u8> {
    let mut seed = client_hello.random.to_vec();
    seed.extend_from_slice(&server_hello.random);
    prf(
        pre_master_secret,
        b"master secret",
        &seed,
        48,
        hasher_factory,
    )
}

/// Standard TLS 1.2 PRF based on the active cipher's hash function.
///
/// PRF(secret, label, seed) = P_<hash>(secret, label + seed)
///
/// TODO: Make me private.
fn prf(
    secret: &[u8],
    label: &[u8],
    seed: &[u8],
    output_size: usize,
    hasher_factory: &HasherFactory,
) -> Vec<u8> {
    let mut data = label.to_vec();
    data.extend_from_slice(seed);

    p_hash(secret, &data, output_size, hasher_factory)
}

/// Defined in thr TLS 1.2 RFC as:
///
/// P_hash(secret, seed) =
///     HMAC_hash(secret, A(1) + seed) +
///     HMAC_hash(secret, A(2) + seed) +
///     HMAC_hash(secret, A(3) + seed) + ...
fn p_hash(
    secret: &[u8],
    seed: &[u8],
    output_size: usize,
    hasher_factory: &HasherFactory,
) -> Vec<u8> {
    // Current value of A(i).
    //
    // We start with the value of A(0) where:
    //   A(0) = seed
    //   A(i) = HMAC_hash(secret, A(i-1))
    let mut a = seed.to_vec();

    let mut out = vec![];
    while out.len() < output_size {
        a = hmac_hash(secret, &a, hasher_factory);

        let mut data = a.clone();
        data.extend_from_slice(seed);

        out.extend_from_slice(&hmac_hash(secret, &data, hasher_factory));
    }

    out.truncate(output_size);

    out
}

fn hmac_hash(secret: &[u8], data: &[u8], hasher_factory: &HasherFactory) -> Vec<u8> {
    let mut hmac = HMAC::new(hasher_factory.box_clone(), secret);
    hmac.update(data);
    hmac.finish()
}
