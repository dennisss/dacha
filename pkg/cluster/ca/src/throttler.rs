use std::time::Duration;

use base_algorithms::token_bucket::TokenBucket;
use crypto::sip::SipHasher;
use crypto::hasher::Hasher;
use crypto::random::SharedRngExt;
use executor::sync::SyncMutex;

/// Roughly a map<key, TokenBucket> used to perform isolated throttled of different entities.
///
/// To avoid actually storing every key since this could be abused, we only keep track of limits
/// per hash bucket of each key (with 'num_buckets' total buckets tracked). 
pub struct HashedTokenBucketThrottler {
    hasher_key: u64,
    buckets: Vec<SyncMutex<TokenBucket>>
}

impl HashedTokenBucketThrottler {
    pub async fn create(
        num_buckets: usize,
        max_tokens: usize,
        refresh_window: Duration
    ) -> Self {
        let hasher_key = crypto::random::global_rng().uniform::<u64>().await;
        
        let mut buckets = vec![];
        for _ in 0..num_buckets {
            buckets.push(SyncMutex::new(TokenBucket::new(max_tokens, refresh_window)));
        }

        Self {
            hasher_key,
            buckets,
        }
    }

    pub fn take(&self, key: &[u8], num: usize) -> bool {
        let bucket = {
            let mut hasher = SipHasher::default_rounds_with_key_halves(self.hasher_key, 0);
            hasher.update(key);
            (hasher.finish_u64() % (self.buckets.len() as u64)) as usize
        };
        
        self.buckets[bucket].apply(|bucket| bucket.take(num)).unwrap()
    }
}
