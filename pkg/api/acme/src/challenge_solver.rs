use common::errors::*;

#[async_trait]
pub trait ACMEChallengeSolver: 'static + Send + Sync {
    fn challenge_type(&self) -> &str;

    /// Solves the challenge and blocks until the ACME server is able to query
    /// for the solution.
    ///
    /// key_authorizations may contain multiple items in cases like
    /// authenticating both 'example.com' and '*.example.com'. If a single session tries
    /// to update multiple of these, then solve_challenge() will only be called once with
    /// all of the requested keys to avoid overwritting data from prior keys.
    async fn solve_challenge(&self, dns_name: &str, key_authorizations: &[String]) -> Result<()>;
}
