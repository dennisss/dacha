use std::future::Future;
use std::sync::Arc;

use common::errors::*;
use executor::cancellation::CancellationToken;

use crate::resource_dependencies::ServiceResourceDependencies;
use crate::{resource::*, CancellationTokenSet, TaskResource};

pub struct ServiceResourceGroup {
    deps: Arc<ServiceResourceDependencies>,
    placeholder_resource: TaskResource,
    // cancellation_tokens: CancellationTokenSet,
}

#[async_trait]
impl ServiceResource for ServiceResourceGroup {
    async fn add_cancellation_token(&self, token: Arc<dyn CancellationToken>) {
        self.placeholder_resource
            .add_cancellation_token(token)
            .await
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
            token.wait_for_cancellation().await;
            deps2
                .update_parent_report(ServiceResourceReport {
                    resource_name: name2,
                    self_state: ServiceResourceState::Done,
                    self_message: None,
                    dependencies: vec![],
                })
                .await;
            Ok(())
        });

        Self {
            deps,
            placeholder_resource,
        }
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
