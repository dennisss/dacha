use std::collections::HashSet;
use std::time::Duration;

use net::backoff::{ExponentialBackoff, ExponentialBackoffOptions};

use crate::status::StatusCode;

#[derive(Clone)]
pub struct RetryingOptions {
    /// TODO: Switch to outbound rate based throttling. We should monitor the
    /// overall error rate and have awareness of backoff signals (like Internal
    /// and ResourceExhausted) (non-backoff signals don't need to count towards
    /// retry rate limits).
    ///
    /// TODO: Also limit max ratio of incoming requests to outgoing requests per
    /// second.
    pub backoff: ExponentialBackoffOptions,

    /// Set of status codes we will retry for requests or methods marked as
    /// idempotent.
    ///
    /// - Non-RPC errors are interprated as having an 'Unknown' code.
    ///     - TODO: Refactor most local errors to InvalidArgument RPC errors.
    /// - Failures will also be retried on all requests if we know for sure that
    ///   that the server has not started processing the request.
    pub retryable_codes: HashSet<StatusCode>,

    /// Maximum number of times we will retry failures that happened locally
    /// (before a request left the local channel / machine).
    pub max_local_error_retries: usize,
}

impl Default for RetryingOptions {
    fn default() -> Self {
        Self {
            backoff: ExponentialBackoffOptions {
                base_duration: Duration::from_millis(1),
                jitter_duration: Duration::from_millis(5),
                max_duration: Duration::from_secs(2),
                cooldown_duration: Duration::from_secs(5),
                max_num_attempts: 3,
            },
            retryable_codes: [StatusCode::Unavailable].into(),
            max_local_error_retries: 2,
        }
    }
}
