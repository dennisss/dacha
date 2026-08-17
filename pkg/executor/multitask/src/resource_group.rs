use std::future::Future;
use std::sync::Arc;

use common::errors::*;
use executor::cancellation::CancellationToken;

use crate::resource_dependencies::ServiceResourceDependencies;
use crate::{resource::*, CancellationTokenSet, TaskResource};

/// A group of ServiceResources (represented as dependencies of the group).
///
/// - Cancellations of the group are immediately propagated to all direct dependencies.
/// - Failures in any dependency triggers cancellation of all dependencies.
pub struct ServiceResourceGroup {
    deps: Arc<ServiceResourceDependencies>,
    placeholder_resource: TaskResource,
}

#[async_trait]
impl ServiceResource for ServiceResourceGroup {
    fn add_cancellation_token(&self, token: Arc<dyn CancellationToken>) {
        self.placeholder_resource
            .add_cancellation_token(token)
    }

    async fn new_resource_subscriber(&self) -> Box<dyn ServiceResourceSubscriber> {
        self.deps.new_resource_subscriber().await
    }
}

impl ServiceResourceGroup {
    pub fn new(name: &str) -> Self {
        let name = name.to_string();

        let deps = Arc::new(ServiceResourceDependencies::new(ServiceResourceReport {
            resource_name: name.clone(),
            self_state: ServiceResourceState::Ready,
            self_message: None,
            dependencies: vec![],
        }));

        let deps2 = deps.clone();
        let name2 = name.to_string();
        let placeholder_resource = TaskResource::spawn(&name, |token| async move {
            Self::watcher_task(name2, token, deps2).await;
            Ok(())
        });

        Self {
            deps,
            placeholder_resource,
        }
    }

    pub(super) async fn watcher_task(
        name: String,
        token: Arc<dyn CancellationToken>,
        deps: Arc<ServiceResourceDependencies>
    ) {
        let mut dep_subscriber = deps.new_resource_subscriber().await;

        loop {
            if token.is_cancelled() {
                break;
            }

            let state = dep_subscriber.value().await.overall_state();
            if state == ServiceResourceState::PermanentFailure {
                break;
            }

            executor::future::race(
                token.wait_for_cancellation(),
                dep_subscriber.wait_for_change()
            ).await;
        }

        // NOTE: This will trigger cancellation of the dependencies in the
        // ServiceResourceDependencies code.
        deps
            .update_parent_report(ServiceResourceReport {
                resource_name: name,
                self_state: ServiceResourceState::Done,
                self_message: None,
                dependencies: vec![],
            })
            .await;
    }

    pub async fn register_dependency(&self, resource: Arc<dyn ServiceResource>) {
        self.deps.register_dependency(resource).await;
    }

    pub async fn spawn<
        F: (FnOnce(Arc<dyn CancellationToken>) -> Fut) + Send + 'static,
        Fut: Future<Output = Result<()>> + Send + 'static,
    >(
        &self,
        name: &str,
        func: F,
    ) -> &Self {
        self.register_dependency(Arc::new(TaskResource::spawn(name, func)))
            .await;
        self
    }

    pub async fn spawn_interruptable<Fut: Future<Output = Result<()>> + Send + 'static>(
        &self,
        name: &str,
        future: Fut,
    ) -> &Self {
        self.register_dependency(Arc::new(TaskResource::spawn_interruptable(name, future)))
            .await;
        self
    }
}
