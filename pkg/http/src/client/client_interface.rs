use std::collections::HashMap;
use std::sync::Arc;

use common::errors::*;
use crypto::hasher::Hasher;
use executor::sync::AsyncMutex;

use crate::request::Request;
use crate::response::Response;

#[derive(Default, Clone)]
pub struct ClientRequestContext {
    pub wait_for_ready: bool,

    /// When using the same http::Client instance, requests with the same key
    /// will try to use the same backend connection.
    ///
    /// NOTE: Affinity is not globally consistent. The same key will resolve to
    /// different backends on different client isntances.
    ///
    /// - By default, consistent hashing is used so keys may shift between
    ///   connections if new keys are added.
    /// - On backend additions/removes/failures, keys may get rebalanced to new
    ///   connections.
    pub affinity: Option<AffinityContext>,
}

#[derive(Clone)]
pub struct AffinityContext {
    pub key: AffinityKey,

    /// If true, then the current request is more tolerant than others to a
    /// connection change.
    ///
    /// TODO: Implement this in the LoadBalancedClient by performing
    /// re-assigments when a backend is being shutdown (grace period between not
    /// wanting a connection to actually triggering a shutdown).
    pub reassignment_tolerant: bool,

    /// If present, then this will be used to store pinned assignments of keys
    /// to connections. When set, we will consume memory that is O(num keys),
    /// but it is guaranteed that we will only snap connections when the
    /// previous connection assigned to a key fails.
    ///
    /// NOTE: If using this, the user that owns this object should clean up
    /// affinity keys after they are done being used.
    pub cache: Option<AffinityKeyCache>,
}

/// Affinity key used to implement 'stickiness' where we keep a set of
/// associated requests (those with the same key) going to the same backend.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
#[repr(transparent)]
pub struct AffinityKey {
    hash: u64,
}

impl AffinityKey {
    pub fn new<T: AsRef<[u8]>>(data: T) -> Self {
        let mut hasher = crypto::sip::SipHasher::default_rounds_with_key_halves(0, 0);
        hasher.update(data.as_ref());
        let hash = hasher.finish_u64();
        Self { hash }
    }

    pub(crate) fn hash(&self) -> u64 {
        self.hash
    }
}

/// Storage used by the internal HTTP client library to keep a memory of where
/// affinity keys have been assigned in the past.
#[derive(Default, Clone)]
pub struct AffinityKeyCache {
    // Currently this is map of affinity_key -> connection_id (where the connection id is the id in
    // the LoadBalancedClient instance).
    storage: Arc<std::sync::Mutex<HashMap<AffinityKey, u64>>>,
}

impl AffinityKeyCache {
    pub(super) fn get(&self, key: AffinityKey) -> Option<u64> {
        self.storage.lock().unwrap().get(&key).cloned()
    }

    pub(super) fn set(&self, key: AffinityKey, value: u64) {
        self.storage.lock().unwrap().insert(key, value);
    }

    pub fn remove(&self, key: AffinityKey) {
        self.storage.lock().unwrap().remove(&key);
    }
}

#[async_trait]
pub trait ClientInterface: 'static + Send + Sync {
    async fn request(
        &self,
        request: Request,
        request_context: ClientRequestContext,
    ) -> Result<Response>;

    async fn current_state(&self) -> ClientState;
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ClientState {
    /// Initial state of the client.
    /// No attempt has been made yet to connect to a remote server so the health
    /// is still unknown but we should start connecting soon.
    Idle,

    Connecting,

    Ready,

    /// There are too many pending requests to this client. New requests will be
    /// instantly rejected.
    Congested,

    /// There was an HTTP/TCP connection level failure and the client needs to
    /// re-connect before handling new requests.
    Failure,

    /// The client is in the process of shutting down (or is already shut down).
    /// There will NOT be further attempts to re-connect.
    Shutdown,
}

impl ClientState {
    /// Returns whether or not the request should be instantly rejected (return
    /// an error).
    ///
    /// TODO: Use this for something.
    pub fn should_reject_request(&self, request_context: &ClientRequestContext) -> bool {
        match *self {
            ClientState::Idle => false,
            ClientState::Connecting => false,
            ClientState::Ready => false,
            ClientState::Congested => true,
            ClientState::Failure => request_context.wait_for_ready,
            ClientState::Shutdown => true,
        }
    }
}

pub trait ClientEventListener: Send + Sync + 'static {
    fn handle_client_state_change(&self);
}
