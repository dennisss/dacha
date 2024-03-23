use std::sync::Arc;

use executor::lock;
use executor::sync::AsyncVariable;

use crate::{
    resource::{ServiceResourceReport, ServiceResourceSubscriber},
    ServiceResource, ServiceResourceState,
};

/// Container for a ServiceResourceReport which is updated over time be the
/// corresponding resource and can be monitored for when it has changed.
pub struct ServiceResourceReportTracker {
    shared: Arc<Shared>,
}

struct Shared {
    /// Contains a monotonic version for the report and the report itself
    value: AsyncVariable<(u64, ServiceResourceReport)>,
}

impl ServiceResourceReportTracker {
    pub fn new(initial_report: ServiceResourceReport) -> Self {
        Self {
            shared: Arc::new(Shared {
                value: AsyncVariable::new((1, initial_report)),
            }),
        }
    }

    pub async fn current_value(&self) -> ServiceResourceReport {
        lock!(v <= self.shared.value.lock().await.unwrap(), {
            v.1.clone()
        })
    }

    pub async fn update(&self, report: ServiceResourceReport) {
        lock!(v <= self.shared.value.lock().await.unwrap(), {
            v.0 += 1;
            v.1 = report;
            v.notify_all();
        });
    }

    pub async fn update_self(
        &self,
        self_state: ServiceResourceState,
        self_message: Option<String>,
    ) {
        lock!(v <= self.shared.value.lock().await.unwrap(), {
            v.0 += 1;
            v.1.self_state = self_state;
            v.1.self_message = self_message;

            if self_state == ServiceResourceState::PermanentFailure {
                eprintln!(
                    "Resource permanent failure: {}: {}",
                    v.1.resource_name,
                    v.1.self_message.as_ref().map(|s| s.as_str()).unwrap_or("")
                );
            }

            v.notify_all();
        });
    }

    pub fn subscribe(&self) -> Box<dyn ServiceResourceSubscriber> {
        Box::new(Subscriber {
            last_version: 0,
            shared: self.shared.clone(),
        })
    }
}

struct Subscriber {
    shared: Arc<Shared>,
    last_version: u64,
}

#[async_trait]
impl ServiceResourceSubscriber for Subscriber {
    async fn wait_for_change(&mut self) {
        loop {
            let v = self.shared.value.lock().await.unwrap().read_exclusive();
            if v.0 > self.last_version {
                break;
            }

            v.wait().await
        }
    }

    async fn value(&mut self) -> ServiceResourceReport {
        lock!(v <= self.shared.value.lock().await.unwrap(), {
            self.last_version = v.0;
            v.1.clone()
        })
    }
}
