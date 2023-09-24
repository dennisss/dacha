use common::errors::*;

#[async_trait]
pub trait ACMEChallengeSolver: 'static + Send + Sync {
    fn challenge_type(&self) -> &str;

    /// Solves the challenge and blocks until the ACME server is able to query
    /// for the solution.
    async fn solve_challenge(&self, dns_name: &str, key_authorization: &str) -> Result<()>;
}
