use std::sync::Arc;

use executor::cancellation::CancellationToken;
use executor::lock;
use executor::sync::AsyncVariable;

/// Collection of of CancellationTokens which must all be cancelled for the
/// overall set to be cancelled.
#[derive(Default)]
pub struct CancellationTokenSet {
    inner: AsyncVariable<Vec<Arc<dyn CancellationToken>>>,
}

impl CancellationTokenSet {
    pub async fn add_cancellation_token(&self, token: Arc<dyn CancellationToken>) {
        lock!(list <= self.inner.lock().await.unwrap(), {
            list.push(token);
            list.notify_all();
        });
    }
}

#[async_trait]
impl CancellationToken for CancellationTokenSet {
    async fn is_cancelled(&self) -> bool {
        let list = self.inner.lock().await.unwrap().read_exclusive();

        if list.is_empty() {
            return false;
        }

        for token in &*list {
            if !token.is_cancelled().await {
                return false;
            }
        }

        true
    }

    async fn wait_for_cancellation(&self) {
        let mut i = 0;

        loop {
            let token = {
                let list = self.inner.lock().await.unwrap().read_exclusive();

                if list.is_empty() {
                    list.wait().await;
                    continue;
                }

                if i >= list.len() {
                    return;
                }

                list[i].clone()
            };

            token.wait_for_cancellation().await;
            i += 1;
        }
    }
}
