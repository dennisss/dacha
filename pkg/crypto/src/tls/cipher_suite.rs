use alloc::boxed::Box;
use std::vec::Vec;

use common::errors::*;

use parsing::binary::be_u16;
use parsing::*;

use crate::aead::AuthEncAD;
use crate::chacha20::ChaCha20Poly1305;
use crate::gcm::AesGCM;
use crate::hasher::GetHasherFactory;
use crate::hasher::HasherFactory;
use crate::sha256::*;
use crate::sha384::*;
use crate::tls::cipher_tls12::*;

// TODO: There's a nice priority list from mozilla here:
// https://wiki.mozilla.org/Security/Cipher_Suites

enum_def_with_unknown!(
    #[allow(non_camel_case_types)] CipherSuite u16 =>
    // TLS 1.3
    TLS_AES_128_GCM_SHA256 = 0x1301,
    TLS_AES_256_GCM_SHA384 = 0x1302,
    TLS_CHACHA20_POLY1305_SHA256 = 0x1303,
    TLS_AES_128_CCM_SHA256 = 0x1304,
    TLS_AES_128_CCM_8_SHA256 = 0x1305,

    // TLS 1.2 : RFC 8422 recommended to mplement
    TLS_ECDHE_RSA_WITH_AES_128_GCM_SHA256 = 0xc02f,
    TLS_ECDHE_RSA_WITH_AES_128_CBC_SHA = 0xc013,
    TLS_ECDHE_ECDSA_WITH_AES_128_GCM_SHA256 = 0xc02b,
    TLS_ECDHE_ECDSA_WITH_AES_128_CBC_SHA = 0xc009,

    TLS_ECDHE_ECDSA_WITH_AES_256_GCM_SHA384 = 0xc02c,
    TLS_ECDHE_ECDSA_WITH_AES_128_CBC_SHA256 = 0xc023,
    TLS_ECDHE_ECDSA_WITH_AES_256_CBC_SHA384 = 0xc024,
    TLS_ECDHE_RSA_WITH_AES_256_GCM_SHA384 = 0xc030,
    TLS_ECDHE_RSA_WITH_AES_128_CBC_SHA256 = 0xc027,
    TLS_ECDHE_RSA_WITH_AES_256_CBC_SHA384 = 0xc028,
    TLS_ECDHE_ECDSA_WITH_CHACHA20_POLY1305_SHA256 = 0xcca9,
    TLS_ECDHE_RSA_WITH_CHACHA20_POLY1305_SHA256 = 0xcca8
);

impl CipherSuite {
    parser!(pub parse<Self> => {
        map(as_bytes(be_u16), |v| Self::from_value(v))
    });

    pub fn serialize(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&self.to_value().to_be_bytes());
    }

    pub fn decode(&self) -> Result<CipherSuiteParts> {
        Ok(match self {
            CipherSuite::TLS_AES_128_GCM_SHA256 => CipherSuiteParts::TLS13(CipherSuiteTLS13::new(
                AesGCM::aes128(),
                SHA256Hasher::factory(),
            )),
            CipherSuite::TLS_AES_256_GCM_SHA384 => CipherSuiteParts::TLS13(CipherSuiteTLS13::new(
                AesGCM::aes256(),
                SHA384Hasher::factory(),
            )),
            CipherSuite::TLS_CHACHA20_POLY1305_SHA256 => CipherSuiteParts::TLS13(
                CipherSuiteTLS13::new(ChaCha20Poly1305::new(), SHA256Hasher::factory()),
            ),
            CipherSuite::TLS_ECDHE_RSA_WITH_AES_128_GCM_SHA256
            | CipherSuite::TLS_ECDHE_ECDSA_WITH_AES_128_GCM_SHA256 => {
                CipherSuiteParts::TLS12(CipherSuiteTLS12 {
                    aead: Box::new(AesGCM::aes128()),
                    nonce_gen: Box::new(GCMNonceGenerator::new()),
                    hasher_factory: SHA256Hasher::factory(),
                })
            }
            CipherSuite::TLS_ECDHE_ECDSA_WITH_CHACHA20_POLY1305_SHA256
            | CipherSuite::TLS_ECDHE_RSA_WITH_CHACHA20_POLY1305_SHA256 => {
                CipherSuiteParts::TLS12(CipherSuiteTLS12 {
                    aead: Box::new(ChaCha20Poly1305::new()),
                    nonce_gen: Box::new(ChaChaPoly1305NonceGenerator::new()),
                    hasher_factory: SHA256Hasher::factory(),
                })
            }
            _ => {
                return Err(err_msg("Bad cipher suite"));
            }
        })
    }
}

pub enum CipherSuiteParts {
    TLS12(CipherSuiteTLS12),
    TLS13(CipherSuiteTLS13),
}

pub struct CipherSuiteTLS12 {
    pub aead: Box<dyn AuthEncAD>,

    pub nonce_gen: Box<dyn NonceGenerator>,

    /// Hasher to use for with the standard TLS 1.2 PRF and for creating the
    /// handshake transcript hash.
    ///
    /// NOTE: We don't currently support any cipher suites with custom PRFs.
    pub hasher_factory: HasherFactory,
}

pub struct CipherSuiteTLS13 {
    pub aead: Box<dyn AuthEncAD>,
    pub hasher_factory: HasherFactory,
}

impl CipherSuiteTLS13 {
    fn new<A: AuthEncAD + 'static>(aead: A, hasher_factory: HasherFactory) -> Self {
        Self {
            aead: Box::new(aead),
            hasher_factory,
        }
    }
}
