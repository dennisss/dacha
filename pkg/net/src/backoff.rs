use std::time::Duration;
use std::time::Instant;

use crypto::random::Rng;
use crypto::random::RngExt;

#[derive(Clone)]
pub struct ExponentialBackoffOptions {
    /// Initial amount of time after which we should retry.
    ///
    /// NOTE: We calculate the waiting duration relative to the completion time
    /// of the last attempt.
    pub base_duration: Duration,

    /// Maximum amount of random noise to add to each retry attempt.
    /// TODO: Also implement this as a percentage of the current
    pub jitter_duration: Duration,

    /// Maximum amount of time to wait before retrying (doesn't include jitter).
    pub max_duration: Duration,

    /// If we see nothing but successful attempts (or nothing happening) for
    /// this amount of time, we wil reset the backoff state.
    pub cooldown_duration: Duration,

    /// Maximum number of attempts allowed.
    ///
    /// - 0 means unlimited attempts.
    /// - 1 means that we will try once and then stop (with no backoff or delays
    ///   at all).
    pub max_num_attempts: usize,
}

pub enum ExponentialBackoffResult {
    Start,
    StartAfter(Duration),
    Stop,
}

pub struct ExponentialBackoff {
    options: ExponentialBackoffOptions,

    /// Current value of 'min(2^n * base_duration, max_duration)'
    current_backoff: Duration,

    /// Oldest time since we haven't had any failures.
    successful_since: Option<Instant>,

    /// Last time we completed an
    last_completion: Option<Instant>,

    attempt_count: usize,

    attempt_pending: bool,

    rng: Box<dyn Rng + Send + Sync + 'static>,
}

impl ExponentialBackoff {
    pub fn new(options: ExponentialBackoffOptions) -> Self {
        Self {
            options,
            current_backoff: Duration::ZERO,
            successful_since: None,
            last_completion: None,
            attempt_pending: false,
            attempt_count: 0,
            rng: Box::new(crypto::random::clocked_rng()),
        }
    }

    pub fn reset(&mut self) {
        *self = Self::new(self.options.clone());
    }

    /// Signals that a new attempt is about to be performed.
    ///
    /// NOTE: We assume that exactly one attempt is being done at a time. New
    /// attempts can't be started until end_attempt() is called on the previous
    /// attempt.
    ///
    /// Returns: The duration that the caller should wait before beginning the
    /// attempt, or None if it's ok to start immediately. This duration
    pub fn start_attempt(&mut self) -> ExponentialBackoffResult {
        if self.attempt_pending {
            self.end_attempt(false);
        }

        if self.options.max_num_attempts > 0 && self.attempt_count >= self.options.max_num_attempts
        {
            return ExponentialBackoffResult::Stop;
        }

        self.attempt_pending = true;

        if self.current_backoff.is_zero() {
            return ExponentialBackoffResult::Start;
        }

        let wait_time = self.current_backoff
            + Duration::from_micros(
                self.rng
                    .between(0, self.options.jitter_duration.as_micros() as u64),
            );

        // If we have already waited a while since the last attempt, then we should be
        // able to safely reduce our wait time.
        let now = Instant::now();
        if let Some(last_completion) = self.last_completion {
            let elapsed = now.duration_since(last_completion);
            if elapsed >= wait_time {
                return ExponentialBackoffResult::Start;
            }

            return ExponentialBackoffResult::StartAfter(wait_time - elapsed);
        }

        ExponentialBackoffResult::StartAfter(wait_time)
    }

    /// Reports that the last attempt was successful.
    pub fn end_attempt(&mut self, successful: bool) {
        let now = Instant::now();
        self.attempt_pending = false;
        self.attempt_count += 1; // TODO: don't overflow here.
        self.last_completion = Some(now);
        if successful {
            let successful_since = self.successful_since.get_or_insert(now);

            if now - *successful_since >= self.options.cooldown_duration {
                self.current_backoff = Duration::ZERO;
            }
        } else {
            if self.current_backoff.is_zero() {
                self.current_backoff = self.options.base_duration;
            } else {
                self.current_backoff =
                    std::cmp::min(2 * self.current_backoff, self.options.max_duration);
            }

            self.successful_since = None;
        }
    }
}
