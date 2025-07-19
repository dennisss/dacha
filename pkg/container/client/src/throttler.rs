use std::time::Duration;
use std::sync::Arc;

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
        max_tokens_per_bucket: usize,
        refresh_window: Duration
    ) -> Self {
        let hasher_key = crypto::random::global_rng().uniform::<u64>().await;
        
        let mut buckets = vec![];
        for _ in 0..num_buckets {
            buckets.push(SyncMutex::new(TokenBucket::new(max_tokens_per_bucket, refresh_window)));
        }

        Self {
            hasher_key,
            buckets,
        }
    }

    pub fn take(&self, key: &[u8], num: usize) -> bool {
        let mut hasher = SipHasher::default_rounds_with_key_halves(self.hasher_key, 0);
        hasher.update(key);
        self.take_impl(hasher.finish_u64(), num)
    }

    pub fn take_with<K: std::hash::Hash>(&self, key: &K, num: usize) -> bool {
        let mut hasher = SipHasher::default_rounds_with_key_halves(self.hasher_key, 0);
        key.hash(&mut hasher);
        self.take_impl(hasher.finish_u64(), num)
    }

    fn take_impl(&self, key_hash: u64, num: usize) -> bool {
        let bucket = (key_hash % (self.buckets.len() as u64)) as usize;
        self.buckets[bucket].apply(|bucket| bucket.take(num)).unwrap()
    }
}

/// For a given hashable key limits the number of concurrent 'tickets' checked out at any time.
pub struct HashedAdmissionLimiter {
    hasher_key: u64,
    shared: Arc<Shared>
}

struct Shared {
    buckets: Vec<SyncMutex<u32>>,
}

impl HashedAdmissionLimiter {
    pub async fn create(
        num_buckets: usize,
        limit_per_bucket: u32,
    ) -> Self {
        let hasher_key = crypto::random::global_rng().uniform::<u64>().await;
        
        let mut buckets = vec![];
        for _ in 0..num_buckets {
            buckets.push(SyncMutex::new(limit_per_bucket));
        }

        Self {
            hasher_key,
            shared: Arc::new(Shared {
                buckets,
            })
        }
    }

    pub fn take_with<K: std::hash::Hash + ?Sized>(&self, key: &K) -> Option<HashedAdmissionLimiterTicket> {
        let mut hasher = SipHasher::default_rounds_with_key_halves(self.hasher_key, 0);
        key.hash(&mut hasher);
        self.take_impl(hasher.finish_u64())
    }

    fn take_impl(&self, key_hash: u64) -> Option<HashedAdmissionLimiterTicket> {
        let bucket = (key_hash % (self.shared.buckets.len() as u64)) as usize;
        let allow = self.shared.buckets[bucket].apply(|bucket| {
            if *bucket == 0 {
                return false;
            }

            *bucket -= 1;
            true
        }).unwrap();

        if allow {
            Some(HashedAdmissionLimiterTicket {
                bucket,
                shared: self.shared.clone()
            })
        } else {
            None
        }
    }
}

pub struct HashedAdmissionLimiterTicket {
    bucket: usize,
    shared: Arc<Shared>
}

impl Drop for HashedAdmissionLimiterTicket {
    fn drop(&mut self) {
        self.shared.buckets[self.bucket].apply(|bucket| {
            *bucket += 1;
        }).unwrap();
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[testcase]
    async fn limiter_works() {
        let limiter = HashedAdmissionLimiter::create(128, 10).await;

        let mut tickets_a = vec![];
        for i in 0..10 {
            tickets_a.push(limiter.take_with("A").unwrap());
        }

        assert!(limiter.take_with("A").is_none());
        assert!(limiter.take_with("A").is_none());

        let mut tickets_b = vec![];
        for i in 0..10 {
            tickets_b.push(limiter.take_with("B").unwrap());
        }

        drop(tickets_a.pop().unwrap());

        assert!(limiter.take_with("A").is_some());
    }
}

