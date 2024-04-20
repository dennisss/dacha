use alloc::boxed::Box;
use std::sync::Arc;

use crate::{lock, sync::AsyncVariable};

/// Object which can be polled to determine if we should stop running some
/// operation.
#[async_trait]
pub trait CancellationToken: 'static + Send + Sync {
    async fn is_cancelled(&self) -> bool;

    async fn wait_for_cancellation(&self);
}

#[derive(Default)]
pub struct TriggerableCancellationToken {
    cancelled: AsyncVariable<bool>,
}

impl TriggerableCancellationToken {
    pub async fn trigger(&self) {
        lock!(cancelled <= self.cancelled.lock().await.unwrap(), {
            *cancelled = true;
            cancelled.notify_all();
        });
    }
}

#[async_trait]
impl CancellationToken for TriggerableCancellationToken {
    async fn is_cancelled(&self) -> bool {
        *self.cancelled.lock().await.unwrap().read_exclusive()
    }

    async fn wait_for_cancellation(&self) {
        loop {
            let cancelled = self.cancelled.lock().await.unwrap().read_exclusive();
            if *cancelled {
                return;
            }

            cancelled.wait().await;
        }
    }
}

#[derive(Default)]
pub struct AlreadyCancelledToken {
    _hidden: (),
}

#[async_trait]
impl CancellationToken for AlreadyCancelledToken {
    async fn is_cancelled(&self) -> bool {
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
    async fn is_cancelled(&self) -> bool {
        self.a.is_cancelled().await || self.b.is_cancelled().await
    }

    async fn wait_for_cancellation(&self) {
        let a = self.a.wait_for_cancellation();
        let b = self.b.wait_for_cancellation();
        crate::future::race(a, b).await
    }
}
