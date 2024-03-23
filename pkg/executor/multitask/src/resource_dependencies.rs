use std::sync::Arc;

use executor::cancellation::TriggerableCancellationToken;
use executor::sync::AsyncVariable;
use executor::{cancellation::CancellationToken, child_task::ChildTask};

use crate::{
    cancellation_token_set::CancellationTokenSet,
    resource::{ServiceResource, ServiceResourceSubscriber},
    resource_report_tracker::ServiceResourceReportTracker,
    ServiceResourceReport, ServiceResourceState,
};

/// Set of resources which are dependencies required for another 'parent'
/// resource to operate.
///
/// This object needs to be continously fed updates to the parent resource's
/// report via update_parent_report (exclusing dependency reports naturally). In
/// exchange the ServiceResourceDependencies fully implements the
/// ServiceResource interface with merging and the parent and dependency reports
/// and:
///
/// - Internally polls dependencies for report/state changes.
/// - Will trigger cancellation of dependencies once the parent resource is
///   terminated.
pub struct ServiceResourceDependencies {
    shared: Arc<Shared>,
}

struct Shared {
    // self_resource: Arc<dyn ServiceResource>,
    // TODO: Child tasks can't be held in Shared since it will a cyclic reference.
    // self_resource_listener: ChildTask,
    dep_cancellation_token: Arc<TriggerableCancellationToken>,
    state: AsyncVariable<State>,

    /// NOTE: A lock on 'state' MUST be held while updating this.
    report: ServiceResourceReportTracker,
}

struct State {
    deps: Vec<(Arc<dyn ServiceResource>, ChildTask)>,
}

impl ServiceResourceDependencies {
    pub fn new(initial_parent_report: ServiceResourceReport) -> Self {
        Self {
            shared: Arc::new(Shared {
                dep_cancellation_token: Arc::new(TriggerableCancellationToken::default()),
                state: AsyncVariable::new(State { deps: vec![] }),
                report: ServiceResourceReportTracker::new(initial_parent_report),
            }),
        }
    }

    pub async fn update_parent_report(&self, parent_report: ServiceResourceReport) {
        let state = self.shared.state.lock().await.unwrap().read_exclusive();

        let mut combined_report = self.shared.report.current_value().await;
        combined_report.resource_name = parent_report.resource_name;
        combined_report.self_state = parent_report.self_state;
        combined_report.self_message = parent_report.self_message;
        assert!(parent_report.dependencies.is_empty());

        // Cancel dependencies when the parent is done.
        if combined_report.self_state.is_terminal() {
            self.shared.dep_cancellation_token.trigger().await;
        }

        self.shared.report.update(combined_report).await;
    }

    pub async fn register_dependency(&self, resource: Arc<dyn ServiceResource>) {
        resource
            .add_cancellation_token(self.shared.dep_cancellation_token.clone())
            .await;

        let shared = self.shared.clone();
        // spawn a new task to make this whole function cancel safe.
        let task = executor::spawn(async move {
            let mut state = shared.state.lock().await.unwrap().enter();

            let idx = state.deps.len();
            let mut resource_sub = resource.new_resource_subscriber().await;

            // Update current state.
            {
                let mut report = shared.report.current_value().await;
                report.dependencies.push(resource_sub.value().await);
                shared.report.update(report).await;
            }

            // Downgrade to avoid a cyclic loop when storing the ChildTask.
            let shared = Arc::downgrade(&shared);

            let listener = ChildTask::spawn(async move {
                loop {
                    resource_sub.wait_for_change().await;

                    let shared = match shared.upgrade() {
                        Some(v) => v,
                        None => return,
                    };

                    // MUST hold a state lock for safely updating the report.
                    let state = shared.state.lock().await.unwrap().read_exclusive();

                    {
                        let mut report = shared.report.current_value().await;
                        report.dependencies[idx] = resource_sub.value().await;
                        shared.report.update(report).await;
                    }
                }
            });

            state.deps.push((resource, listener));

            state.exit();
        });

        task.join().await;
    }

    pub async fn new_resource_subscriber(&self) -> Box<dyn ServiceResourceSubscriber> {
        self.shared.report.subscribe()
    }
}
