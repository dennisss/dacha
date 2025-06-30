use std::future::Future;
use std::sync::Arc;

use common::errors::*;
use executor::cancellation::CancellationToken;

use crate::resource_dependencies::ServiceResourceDependencies;
use crate::{resource::*, TaskResource, ServiceResourceGroup};

/// Top level resource that represents an entire program.
///
/// - There should just exist a single instance of this per program run (normally created in the main() function).
/// - All other resources needed by the program should be added as children of this resource.
pub struct RootResource {
    deps: Arc<ServiceResourceDependencies>,
}

impl RootResource {
    pub fn new() -> Self {
        let deps = Arc::new(ServiceResourceDependencies::new(ServiceResourceReport {
            resource_name: "Root".to_string(),
            self_state: ServiceResourceState::Ready,
            self_message: None,
            dependencies: vec![],
        }));

        let deps2 = deps.clone();
        // TODO: Make this smart enough to change its behavior to the end of unit tests
        // when those happen.
        let cancellation_token = executor::signals::new_shutdown_token();
        executor::spawn(async move {
            ServiceResourceGroup::watcher_task("Root".to_string(), cancellation_token, deps2).await;
        });

        Self { deps }
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

    /// Waits until we have reached a terminal state for the resources.
    pub async fn wait(&self) -> Result<()> {
        let mut subscriber = self.deps.new_resource_subscriber().await;
        wait_for_termination(subscriber, false).await
    }
}

pub async fn wait_for_main_resource<R: ServiceResource>(resource: R) -> Result<()> {
    let root = RootResource::new();
    root.register_dependency(Arc::new(resource)).await;
    root.wait().await
}
