use crypto::sha256::SHA256Hasher;
use crypto::hasher::Hasher;
use crypto::random::{SharedRng, SharedRngExt};

pub const AUTH_KEY_LEN: usize = 16;

const AUTH_KEY_HASH_LEN: usize = 16;

/// Header returned by the auth job to the frontend to signal that a new auth
/// key value should be passed back to the end user.
///
/// This may be equal to the special AUTH_KEY_DELETED_VALUE value which should
/// cause the client to forget the current auth key.
pub const AUTH_KEY_HEADER: &'static str = "X-Auth-Key";

pub const AUTH_KEY_DELETED_VALUE: &'static str = "deleted";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(transparent)]
pub struct ClientId(pub u64);

impl ClientId {
    pub async fn generate() -> Self {
        Self(crypto::random::global_rng().uniform::<u64>().await)
    }

    pub fn to_string(&self) -> String {
        base_radix::base64url_encode(&self.0.to_be_bytes())
    }
}

pub async fn generate_session_id() -> u64 {
    crypto::random::global_rng().uniform::<u64>().await
}

pub async fn generate_session_auth_key() -> Vec<u8> {
    let mut data = vec![0u8; AUTH_KEY_LEN];
    crypto::random::global_rng().generate_bytes(&mut data).await;
    data
}

pub fn create_session_auth_key_hash(auth_key: &[u8]) -> Vec<u8> {
    let mut h = SHA256Hasher::default();
    h.update(auth_key);
    let mut out = h.finish();
    out.truncate(AUTH_KEY_HASH_LEN); 
    out
}
