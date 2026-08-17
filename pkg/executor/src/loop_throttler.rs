use std::time::{Duration, Instant};

use base_algorithms::token_bucket::TokenBucket;

use crate::cancellation::CancellationToken;

/// Throttler for loops to protect against infinite looping.
///
/// Internally this uses a token bucket based throttling approach to rate limit
/// each loop iteration.
pub struct LoopThrottler {
    bucket: TokenBucket
}

impl LoopThrottler {
    pub fn new(max_tokens: usize, refresh_window: Duration) -> Self {
        Self {
            bucket: TokenBucket::new(max_tokens, refresh_window)
        }
    }

    pub async fn start_iteration(&mut self) {
        let mut did_sleep = false;
        loop {
            if !self.bucket.take(1) {
                crate::sleep(self.bucket.time_per_token()).await;
                did_sleep = true;
                continue;
            }

            if !did_sleep {
                crate::yield_now().await;
            }

            break;
        }
    }

    /// Returns whether or not the loop should execute.
    pub async fn start_cancellable_iteration(
        &mut self,
        cancellation_token: &dyn CancellationToken,
    ) -> bool {
        crate::future::race(
            self.start_iteration(),
            cancellation_token.wait_for_cancellation(),
        )
        .await;

        !cancellation_token.is_cancelled()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use base_error::*;

    #[test]
    fn loop_throttler_works() -> Result<()> {
        crate::run(async move {
            let mut throttler = LoopThrottler::new(10, Duration::from_secs(1));

            let mut start = Instant::now();
            for i in 0..100 {
                throttler.start_iteration().await;
            }

            let mut end = Instant::now();

            let t = end - start;
            assert!(t >= Duration::from_secs(8) && t < Duration::from_secs(11));

            Ok(())
        })?
    }
}
