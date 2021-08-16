use common::errors::*;

/// NOTE: This is not a public/private crypto system in itself.
#[async_trait]
pub trait DiffieHellmanFn: Send + Sync {
    /// Generates a secret value for this function. This value is expected to
    /// never to sent to another agent.
    async fn secret_value(&self) -> Result<Vec<u8>>;

    /// For a secret value, produces the corresponding public value which *can*
    /// be safely sent to another agent.
    fn public_value(&self, secret: &[u8]) -> Result<Vec<u8>>;

    /// Given our secret and some other agent's public value, produces a new
    /// shared secret value known to both parties.
    ///
    /// This may
    fn shared_secret(&self, secret: &[u8], public: &[u8]) -> Result<Vec<u8>>;
}
