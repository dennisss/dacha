use std::future::Future;
use std::sync::Arc;

use common::errors::*;
use executor::cancellation::{AlreadyCancelledToken, CancellationToken};

use crate::cancellation_token_set::CancellationTokenSet;
use crate::resource_dependencies::ServiceResourceDependencies;
use crate::resource_report_tracker::ServiceResourceReportTracker;
use crate::{
    ServiceResource, ServiceResourceReport, ServiceResourceState, ServiceResourceSubscriber,
};

/// A resource which is implemented as a single task.
pub struct TaskResource {
    shared: Arc<Shared>,
}

struct Shared {
    report: ServiceResourceReportTracker,
    cancellation_tokens: Arc<CancellationTokenSet>,
}

// TODO: Maybe implement this as part of the CancellationTokenSet drop
impl Drop for TaskResource {
    fn drop(&mut self) {
        let shared = self.shared.clone();
        executor::spawn(async move {
            shared
                .cancellation_tokens
                .add_cancellation_token(Arc::new(AlreadyCancelledToken::default()))
                .await
        });
    }
}

impl TaskResource {
    pub fn spawn<
        F: (FnOnce(Arc<dyn CancellationToken>) -> Fut) + Send + 'static,
        Fut: Future<Output = Result<()>> + Send + 'static,
    >(
        name: &str,
        func: F,
    ) -> Self {
        let initial_report = ServiceResourceReport {
            resource_name: name.to_string(),
            self_state: ServiceResourceState::Ready,
            self_message: None,
            dependencies: vec![],
        };

        let shared = Arc::new(Shared {
            report: ServiceResourceReportTracker::new(initial_report.clone()),
            cancellation_tokens: Arc::new(CancellationTokenSet::default()),
        });

        let shared2 = shared.clone();
        executor::spawn(async move {
            let r = func(shared2.cancellation_tokens.clone()).await;

            let mut message = None;
            let state = match r {
                Ok(()) => ServiceResourceState::Done,
                Err(e) => {
                    message = Some(e.to_string());
                    ServiceResourceState::PermanentFailure
                }
            };

            let new_report = ServiceResourceReport {
                resource_name: initial_report.resource_name.clone(),
                self_state: state,
                self_message: message,
                dependencies: vec![],
            };

            shared2.report.update(new_report).await;
        });

        Self { shared }
    }

    pub fn spawn_interruptable<Fut: Future<Output = Result<()>> + Send + 'static>(
        name: &str,
        future: Fut,
    ) -> Self {
        Self::spawn(name, move |token| async move {
            executor::future::race(
                executor::future::map(token.wait_for_cancellation(), |()| Ok(())),
                future,
            )
            .await
        })
    }
}

#[async_trait]
impl ServiceResource for TaskResource {
    async fn add_cancellation_token(&self, token: Arc<dyn CancellationToken>) {
        self.shared
            .cancellation_tokens
            .add_cancellation_token(token)
            .await;
    }

    async fn new_resource_subscriber(&self) -> Box<dyn ServiceResourceSubscriber> {
        self.shared.report.subscribe()
    }
}
