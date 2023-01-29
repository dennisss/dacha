use alloc::boxed::Box;

/// Object which can be polled to determine if we should stop running some
/// operation.
#[async_trait]
pub trait CancellationToken: 'static + Send + Sync {
    fn is_cancelled(&self) -> bool;

    async fn wait_for_cancellation(&self);
}
