use common::async_std::sync::Mutex;
use common::bytes::Bytes;

use crate::aead::AuthEncAD;
use crate::tls::key_schedule::*;
use crate::hkdf::HKDF;

/// Defines how to encrypt/decrypt data on a TLS connection.
///
/// This is negotiated during the TLS handshake and defines which algorithm to use
/// for encryption, what keys are currently in play, and how the keys will change
/// in the future.
pub struct CipherSpec {
    pub aead: Box<dyn AuthEncAD>,
    hkdf: HKDF,

    pub client_key: Mutex<CipherEndpointKey>,
    pub server_key: Mutex<CipherEndpointKey>,
}

impl CipherSpec {
    pub fn from_keys(
        aead: Box<dyn AuthEncAD>,
        hkdf: HKDF,
        client_traffic_secret: Bytes,
        server_traffic_secret: Bytes,
    ) -> Self {
        // TODO: This is very redundant with replace_keys
        let client_key = Mutex::new(CipherEndpointKey::from_key(
            aead.as_ref(),
            &hkdf,
            client_traffic_secret,
        ));
        let server_key = Mutex::new(CipherEndpointKey::from_key(
            aead.as_ref(),
            &hkdf,
            server_traffic_secret,
        ));

        Self {
            aead,
            hkdf,
            client_key,
            server_key,
        }
    }

    // TODO: If there are multiple readers, then this must always occur
    // during the same locking cycle as the
    // TODO: Only valid for application keys and not for handshake keys.

    pub async fn replace_keys(
        &mut self,
        client_traffic_secret: Bytes,
        server_traffic_secret: Bytes,
    ) {
        *self.client_key.lock().await =
            CipherEndpointKey::from_key(self.aead.as_ref(), &self.hkdf, client_traffic_secret);
        *self.server_key.lock().await =
            CipherEndpointKey::from_key(self.aead.as_ref(), &self.hkdf, server_traffic_secret);
    }
}

pub struct CipherEndpointKey {
    traffic_secret: Bytes,
    
    /// Derived from the above key.
    pub keying: TrafficKeyingMaterial,
}

impl CipherEndpointKey {
    fn from_key(aead: &dyn AuthEncAD, hkdf: &HKDF, traffic_secret: Bytes) -> Self {
        let keying = TrafficKeyingMaterial::from_secret(hkdf, aead, &traffic_secret);
        Self {
            traffic_secret,
            keying,
        }
    }

    // NOTE: This must be called under the same lock that sent/received the key
    // change request to ensure no other messages are received/send under the
    // old keys.
    //
    // application_traffic_secret_N+1 =
    //        HKDF-Expand-Label(application_traffic_secret_N,
    //                          "traffic upd", "", Hash.length)
    fn update_key(&mut self, aead: &dyn AuthEncAD, hkdf: &HKDF) {
        let next_secret = hkdf_expand_label(
            &hkdf,
            &self.traffic_secret,
            b"traffic upd",
            b"",
            hkdf.hash_size() as u16,
        )
        .into();

        *self = Self::from_key(aead, hkdf, next_secret);
    }
}