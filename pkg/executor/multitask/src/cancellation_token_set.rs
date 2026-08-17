use std::sync::Arc;

use executor::cancellation::CancellationToken;
use executor::lock;
use executor::sync::{SyncMutex, Waiters};
use executor::channel::oneshot;

/// Collection of of CancellationTokens which must all be cancelled for the
/// overall set to be cancelled.
#[derive(Default)]
pub struct CancellationTokenSet {
    inner: SyncMutex<State>,
}

#[derive(Default)]
struct State {
    tokens: Vec<Arc<dyn CancellationToken>>,
    waiters: Waiters,
}

impl CancellationTokenSet {
    pub fn add_cancellation_token(&self, token: Arc<dyn CancellationToken>) {
        self.inner.apply(|state| {
            state.tokens.push(token);
            state.waiters.notify_all();
        }).unwrap();
    }
}

#[async_trait]
impl CancellationToken for CancellationTokenSet {
    fn is_cancelled(&self) -> bool {
        self.inner.apply(|state| {
            if state.tokens.is_empty() {
                return false;
            }

            for token in &state.tokens {
                if !token.is_cancelled() {
                    return false;
                }
            }

            true
        }).unwrap()
    }

    async fn wait_for_cancellation(&self) {
        let mut i = 0;

        loop {
            let (token, waiter) = self.inner.apply(|state| {
                if state.tokens.is_empty() {
                    return (None, Some(state.waiters.new_waiter()));
                }

                if i >= state.tokens.len() {
                    // Done
                    return (None, None);
                }

                (Some(state.tokens[i].clone()), None)
            }).unwrap();

            if let Some(w) = waiter {
                w.await;
                continue;
            }

            let token = match token {
                Some(v) => v,
                None => break
            };

            token.wait_for_cancellation().await;
            i += 1;
        }
    }
}
