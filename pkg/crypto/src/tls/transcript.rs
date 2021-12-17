use std::vec::Vec;

use common::bytes::Bytes;

use crate::hasher::*;

/// Stores a list of all handshake messages seen as part of the TLS handshake.
pub struct Transcript {
    messages: Vec<Bytes>,
}

impl Transcript {
    pub fn new() -> Self {
        Self { messages: vec![] }
    }

    pub fn push(&mut self, message: Bytes) {
        self.messages.push(message);
    }

    pub fn clear(&mut self) {
        self.messages.clear();
    }

    /// Computes the hash of all messages seen,
    ///
    /// TODO: Implement a rolling hash. But the transcript should still be
    /// initializable without a hash.
    pub fn hash(&self, hasher_factory: &HasherFactory) -> Vec<u8> {
        let mut hasher = hasher_factory.create();
        for m in self.messages.iter() {
            hasher.update(&m);
        }

        hasher.finish()
    }
}
