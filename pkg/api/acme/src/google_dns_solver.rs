use std::time::Duration;

use common::errors::*;
use crypto::{hasher::Hasher, sha256::SHA256Hasher};

use crate::{ACMEChallengeSolver, DNS_01};

pub struct GoogleDNSSolver {
    client: google_dns::Client,
}

impl GoogleDNSSolver {
    pub fn new(client: google_dns::Client) -> Self {
        Self { client }
    }
}

#[async_trait]
impl ACMEChallengeSolver for GoogleDNSSolver {
    fn challenge_type(&self) -> &str {
        DNS_01
    }

    async fn solve_challenge(&self, dns_name: &str, key_authorization: &str) -> Result<()> {
        let record_name = format!("_acme-challenge.{}.", dns_name);

        let data = {
            let mut hasher = SHA256Hasher::default();
            hasher.update(key_authorization.as_bytes());
            base_radix::base64url_encode(&hasher.finish())
        };

        self.client
            .set_txt_record(&record_name, 300, &[data])
            .await?;

        // Wait for DNS propagation delay.
        // TODO: Make this 2 minutes if we implement direct authoritative server
        // checking.
        executor::sleep(Duration::from_secs(4 * 60)).await?;

        Ok(())
    }
}
