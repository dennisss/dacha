use alloc::boxed::Box;

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
