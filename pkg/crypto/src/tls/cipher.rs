use common::async_std::sync::Mutex;
use common::bytes::Bytes;

use crate::aead::AuthEncAD;
use crate::tls::key_schedule::*;
use crate::hkdf::HKDF;

/// Defines how to encrypt/decrypt data on one half of a TLS connection.
///
/// This is negotiated during the TLS handshake and defines which algorithm to use
/// for encryption, what keys are currently in play, and how the keys will change
/// in the future.
///
/// While this only defines one half of the keys in the connection, the other side
/// will almost always be using the same AEAD and HKDF config.
pub struct CipherEndpointSpec {
    pub aead: Box<dyn AuthEncAD>,
    
    hkdf: HKDF,

    traffic_secret: Bytes,
    
    /// Derived from the above traffic secret.
    ///
    /// TODO: Make this private.
    pub keying: TrafficKeyingMaterial,
}

impl CipherEndpointSpec {
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
        self.keying = TrafficKeyingMaterial::from_secret(&self.hkdf, self.aead.as_ref(), &traffic_secret);
        self.traffic_secret = traffic_secret;
    }

    // NOTE: This must be called under the same lock that sent/received the key
    // change request to ensure no other messages are received/send under the
    // old keys.
    //
    // NOTE: It's only valid to call this after the TLS handshake.
    //
    // application_traffic_secret_N+1 =
    //        HKDF-Expand-Label(application_traffic_secret_N,
    //                          "traffic upd", "", Hash.length)
    pub fn update_key(&mut self, aead: &dyn AuthEncAD, hkdf: &HKDF) {
        let next_secret = hkdf_expand_label(
            &hkdf,
            &self.traffic_secret,
            b"traffic upd",
            b"",
            hkdf.hash_size() as u16,
        )
        .into();

        self.replace_key(next_secret);
    }
}