#[async_trait]
pub trait CancellationToken: 'static + Send + Sync {
    async fn wait(&self);
}