use alloc::boxed::Box;

/// Object which can be polled to determine if we should stop running some
/// operation.
#[async_trait]
pub trait CancellationToken: 'static + Send + Sync {
    async fn wait(&self);
}
