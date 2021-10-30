use std::time::Duration;
use std::time::Instant;

use crypto::random::Rng;
use crypto::random::RngExt;

#[derive(Clone)]
pub struct ExponentialBackoffOptions {
    /// Initial amount of time after which we should retry.
    pub base_duration: Duration,

    /// Maximum amount of random noise to add to each retry attempt.
    /// TODO: Also implement this as a percentage of the current
    pub jitter_duration: Duration,

    /// Maximum amount of time to wait before retrying.
    pub max_duration: Duration,

    /// Amount of
    pub cooldown_duration: Duration,
}

pub struct ExponentialBackoff {
    options: ExponentialBackoffOptions,

    /// Number of consecutive failures we've had so
    failure_count: usize,

    /// Oldest time since we haven't had any failures.
    successful_since: Option<Instant>,

    attempt_pending: bool,

    rng: Box<dyn Rng + Send + 'static>,
}

impl ExponentialBackoff {
    pub fn new(options: ExponentialBackoffOptions) -> Self {
        Self {
            options,
            failure_count: 0,
            successful_since: None,
            attempt_pending: false,
            rng: Box::new(crypto::random::clocked_rng()),
        }
    }

    /// Signals that a new attempt is about to be performed.
    ///
    /// TODO: Return an absolute duration which is relative to the start time of
    /// the last attempt.
    ///
    /// Returns: The duration that the caller should wait before beginning the
    /// attempt, or None if it's ok to start immediately.
    pub fn start_attempt(&mut self) -> Option<Duration> {
        if self.attempt_pending {
            self.end_attempt(false);
        }
        self.attempt_pending = true;

        if self.failure_count == 0 {
            return None;
        }

        let wait_time = Duration::from_micros(
            2u64.pow(self.failure_count as u32 - 1)
                * (self.options.base_duration.as_micros() as u64)
                + self
                    .rng
                    .between(0, self.options.jitter_duration.as_micros() as u64),
        );

        Some(wait_time)
    }

    /// Reports that a recent attempt was successful.
    /// This should be called frequently to ensure
    pub fn end_attempt(&mut self, successful: bool) {
        self.attempt_pending = false;
        if successful {
            let now = Instant::now();
            let successful_since = self.successful_since.get_or_insert(now);

            if now - *successful_since >= self.options.cooldown_duration {
                self.failure_count = 0;
            }
        } else {
            self.failure_count += 1;
            self.successful_since = None;
        }
    }
}
