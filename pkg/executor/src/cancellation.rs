use alloc::boxed::Box;
use std::sync::Arc;

use crate::lock;
use crate::sync::{SyncMutex, Waiters};

/// Object which can be polled to determine if we should stop running some
/// operation.
#[async_trait]
pub trait CancellationToken: 'static + Send + Sync {
    fn is_cancelled(&self) -> bool;

    async fn wait_for_cancellation(&self);
}

/// A cancellation token which is only marked as cancelled when a user manually
/// runs the trigger() function on it.
#[derive(Default)]
pub struct TriggerableCancellationToken {
    state: SyncMutex<TriggerState>
}

#[derive(Default)]
struct TriggerState {
    cancelled: bool,
    waiters: Waiters,
}

impl TriggerableCancellationToken {
    pub fn trigger(&self) {
        self.state.apply(|state| {
            state.cancelled = true;
            state.waiters.notify_all();
        }).unwrap()
    }
}

#[async_trait]
impl CancellationToken for TriggerableCancellationToken {
    fn is_cancelled(&self) -> bool {
        self.state.apply(|state| state.cancelled).unwrap()
    }

    async fn wait_for_cancellation(&self) {
        loop {
            let waiter = self.state.apply(|state| {
                if state.cancelled {
                    return None;
                }

                Some(state.waiters.new_waiter())
            }).unwrap();

            if let Some(waiter) = waiter {
                waiter.await;
            } else {
                break;
            }
        }
    }
}

/// A cancellation token which is already cancelled.
#[derive(Default)]
pub struct AlreadyCancelledToken {
    _hidden: (),
}

#[async_trait]
impl CancellationToken for AlreadyCancelledToken {
    fn is_cancelled(&self) -> bool {
        true
    }

    async fn wait_for_cancellation(&self) {}
}

/// A cancellation token which is cancelled when either of two inner tokens are
/// cancelled.
pub struct EitherCancelledToken {
    a: Arc<dyn CancellationToken>,
    b: Arc<dyn CancellationToken>,
}

impl EitherCancelledToken {
    pub fn new(a: Arc<dyn CancellationToken>, b: Arc<dyn CancellationToken>) -> Self {
        Self { a, b }
    }
}

#[async_trait]
impl CancellationToken for EitherCancelledToken {
    fn is_cancelled(&self) -> bool {
        self.a.is_cancelled() || self.b.is_cancelled()
    }

    async fn wait_for_cancellation(&self) {
        let a = self.a.wait_for_cancellation();
        let b = self.b.wait_for_cancellation();
        crate::future::race(a, b).await
    }
}
