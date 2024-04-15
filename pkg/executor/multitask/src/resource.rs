use std::fmt::Debug;
use std::sync::Arc;

use common::{errors::*, line_builder::LineBuilder};
use executor::{cancellation::CancellationToken, sync::SyncMutex};

/*
Some problems with this:
- If a resource is added as a dependency of a fast running resource, then it risks being cancelled before it can be added as a dependency of other resources.
- If a user forgets that some object implements 'ServiceResource', then they may not add it to the resource tree and have it get tracked.
*/

///
#[async_trait]
pub trait ServiceResource: 'static + Send + Sync {
    /// Registers a cancellation token which should be watched to determine when
    /// the resource should start shutting down.
    ///
    /// If a resource has >= 1 cancellation tokens and ALL of them are marked as
    /// cancelled, then it should automatically trigger shutdown.
    ///
    /// NOTE: This is automatically called internally in ServiceResourceGroup.
    async fn add_cancellation_token(&self, token: Arc<dyn CancellationToken>);

    /// Creates a subscriber which can be used to read the current state of the
    /// resource and wait for future changes.
    async fn new_resource_subscriber(&self) -> Box<dyn ServiceResourceSubscriber>;

    /*
    /// Called (by RootResource::wait()) to indicate that the service has been initialized.
    /// After this point it is safe to assume that the cancellation/dependency tree is finalized.
    async fn on_service_initialized(&self);
    */

    async fn wait_for_termination(&self) -> Result<()> {
        let mut subscriber = self.new_resource_subscriber().await;
        wait_for_termination(subscriber).await
    }
}

pub(crate) async fn wait_for_termination(
    mut subscriber: Box<dyn ServiceResourceSubscriber>,
) -> Result<()> {
    loop {
        let report = subscriber.value().await;
        let (state, message) = report.overall_state_and_message();

        // println!("===============");
        // println!("{:?}", report);

        match state {
            ServiceResourceState::PermanentFailure => {
                // TODO: It may be from an error message other than self
                return Err(format_err!("Resource failed: {}", message.unwrap_or("")));
            }
            ServiceResourceState::Done => {
                return Ok(());
            }
            _ => {
                subscriber.wait_for_change().await;
            }
        }
    }
}

impl dyn ServiceResource {
    pub async fn wait_for_ready(&self) {
        let mut sub = self.new_resource_subscriber().await;
        loop {
            let report = sub.value().await;
            if report.overall_state() == ServiceResourceState::Ready {
                break;
            }

            sub.wait_for_change().await;
        }
    }
}

#[async_trait]
pub trait ServiceResourceSubscriber: 'static + Send + Sync {
    async fn wait_for_change(&mut self);

    async fn value(&mut self) -> ServiceResourceReport;
}

#[derive(Clone)]
pub struct ServiceResourceReport {
    pub resource_name: String,
    pub self_state: ServiceResourceState,
    pub self_message: Option<String>,
    pub dependencies: Vec<ServiceResourceReport>,
}

impl ServiceResourceReport {
    pub fn overall_state(&self) -> ServiceResourceState {
        self.overall_state_and_message().0
    }

    pub fn overall_state_and_message(&self) -> (ServiceResourceState, Option<&str>) {
        let mut state = self.self_state;
        let mut message = self.self_message.as_ref().map(|s| s.as_str());
        for dep in &self.dependencies {
            let (s, m) = dep.overall_state_and_message();
            state = state.merge(s);
            if s == state {
                message = m;
            }
        }

        (state, message)
    }

    fn to_string(&self, out: &mut LineBuilder) {
        out.add(format!(
            "- {}: {:?} {}",
            self.resource_name,
            self.self_state,
            self.self_message.as_ref().unwrap_or(&String::new())
        ));

        out.indented(|out| {
            for dep in &self.dependencies {
                dep.to_string(out);
            }
        });
    }
}

impl Debug for ServiceResourceReport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut lines = LineBuilder::new();
        self.to_string(&mut lines);

        write!(f, "{}", lines.to_string())
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum ServiceResourceState {
    /// The resource is still running initial startup logic and isn't available
    /// for usage yet.
    Loading,

    /// The resource is fully loaded and ready for usage immediately.
    Ready,

    /// The resource received a shutdown/cancellation signal so is gracefully
    /// cleaning up internal state.
    Stopping,

    /// The resource is currently unavailable but may become healthy again soon.
    TemporaryFailure,

    /// The resource experienced a fatal error and has completely stopped
    /// running.
    PermanentFailure,

    /// All background tasks for this resource have finished successfully
    /// (either due to shutdown or ).
    Done,
}

impl ServiceResourceState {
    pub fn merge(&self, other: Self) -> Self {
        if *self == Self::PermanentFailure || other == Self::PermanentFailure {
            return Self::PermanentFailure;
        }

        if *self == Self::TemporaryFailure || other == Self::TemporaryFailure {
            return Self::TemporaryFailure;
        }

        if *self == Self::Stopping || other == Self::Stopping {
            return Self::Stopping;
        }

        if *self == Self::Loading || other == Self::Loading {
            return Self::Loading;
        }

        if *self == Self::Done && other == Self::Done {
            return Self::Done;
        }

        // NOTE: If one of them is Ready and the other is Done, we will just mark return
        // Ready.

        Self::Ready
    }

    pub fn is_terminal(&self) -> bool {
        match self {
            ServiceResourceState::Loading
            | ServiceResourceState::Ready
            | ServiceResourceState::Stopping
            | ServiceResourceState::TemporaryFailure => false,
            ServiceResourceState::PermanentFailure | ServiceResourceState::Done => true,
        }
    }
}
