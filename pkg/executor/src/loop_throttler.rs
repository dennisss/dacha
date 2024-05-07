use std::time::{Duration, Instant};

use crate::cancellation::CancellationToken;

/// Throttler for loops to protect against infinite looping.
///
/// Internally this uses a token bucket based throttling approach to rate limit
/// each loop iteration.
pub struct LoopThrottler {
    remaining_tokens: usize,
    max_tokens: usize,
    time_per_token: Duration,
    last_time: Instant,
}

impl LoopThrottler {
    pub fn new(max_tokens: usize, refresh_window: Duration) -> Self {
        let time_per_token = refresh_window / (max_tokens as u32);

        Self {
            remaining_tokens: max_tokens,
            max_tokens,
            time_per_token,
            last_time: Instant::now(),
        }
    }

    pub async fn start_iteration(&mut self) {
        let mut did_sleep = false;
        loop {
            // Update number of remaining tokens.
            let now = Instant::now();
            let increment = ((now - self.last_time).as_micros() as u64)
                / (self.time_per_token.as_micros() as u64);
            self.last_time += self.time_per_token * (increment as u32);
            self.remaining_tokens = core::cmp::min(
                self.max_tokens,
                self.remaining_tokens + (increment as usize),
            );

            if self.remaining_tokens == 0 {
                crate::sleep(self.time_per_token).await;
                did_sleep = true;
                continue;
            }

            if !did_sleep {
                crate::yield_now().await;
            }

            self.remaining_tokens -= 1;
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

        !cancellation_token.is_cancelled().await
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
