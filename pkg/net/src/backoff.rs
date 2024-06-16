use alloc::boxed::Box;
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
    /// More specifically, this is the maximum number of attempts since the last
    /// successful one.
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

/// Tracker for how long a user should backoff between attempts to execute some
/// request/operation.
///
/// Each ExponentialBackoff instance should only be used for sequential
/// attempts.
/// - Don't use to throttle requests being made in parallel (retry each request
///   with a separate ExponentialBackoff instance/loop)
/// - If doing a continuous operation like maintaining a single connection to a
///   backend, then having one instance is appropriate.
///
/// The typical request retry loop should look like:
/// 1. Create am ExponentialBackoff instance.
/// 2. Call start_attempt() and wait if requested.
/// 3. Execute the request/operation.
/// 4. Call end_attempt(success)
/// 5. If successful, return to the caller, else, repeat at step #2.
///
/// If you expect to make many requests back to back (e.g. long polling):
/// 1. Create am ExponentialBackoff instance.
/// 2. Call start_attempt() and wait if requested.
/// 3. Execute the request/operation.
///    - If there is a sign of success (e.g. bytes were returned, call
///      end_attempt(true)).
/// 4. Call end_attempt(final_success)
/// 5. Loop back to step #2.
///
/// Note that in the above example, end_attempt could be called multiple times
/// per attempt (indicating that the attempt eventually failed after some chunks
/// returning successful).
pub struct ExponentialBackoff {
    options: ExponentialBackoffOptions,

    /// Current value of 'min(2^n * base_duration, max_duration)'
    current_backoff: Duration,

    /// Oldest time since we haven't had any failures.
    successful_since: Option<Instant>,

    /// Time at which the last attempt was completed.
    last_completion: Option<Instant>,

    /// Total number of completed attempts.
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
        // In case the user forgot to call end_attempt(), call it assuming the request
        // failed.
        if self.attempt_pending {
            self.end_attempt(false);
        }

        if self.options.max_num_attempts > 0 && self.attempt_count >= self.options.max_num_attempts
        {
            return ExponentialBackoffResult::Stop;
        }

        self.attempt_pending = true;
        if self.options.max_num_attempts > 0 {
            self.attempt_count += 1;
        }

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

    /// Reports whether or not the last attempt was successful.
    ///
    /// This is allowed to be called multiple times if the status of the attempt
    /// changes from successful to failing (this is mainly to be used for long
    /// running streams which respond in chunks).
    pub fn end_attempt(&mut self, successful: bool) {
        let now = Instant::now();
        self.attempt_pending = false;
        self.last_completion = Some(now);

        if let Some(successful_since) = &self.successful_since {
            if now - *successful_since > self.options.cooldown_duration {
                self.current_backoff = Duration::ZERO;
            }
        }

        if successful {
            self.attempt_count = 0;
            self.successful_since.get_or_insert(now);
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
