use std::sync::Arc;
use std::time::{Duration, Instant};
use std::collections::{HashMap, HashSet};

use common::errors::*;
use executor::{lock_async, lock};
use executor::sync::AsyncMutex;
use protobuf::Message;
use executor::sync::AsyncVariable;
use ptp_proto::ptp::*;

pub struct DummyTimeSyncNode {
    state: AsyncMutex<State>
}

struct State {
    config: TimeSyncConfig,
    last_configured: Instant,
}

impl DummyTimeSyncNode {
    pub fn create() -> Self {
        Self {
            state: AsyncMutex::new(State {
                config: TimeSyncConfig::default(),
                last_configured: Instant::now()
            })
        }
    }
} 

#[async_trait]
impl TimeSyncService for DummyTimeSyncNode {
    async fn Status(
        &self,
        request: rpc::ServerRequest<StatusRequest>,
        response: &mut rpc::ServerResponse<StatusResponse>
    ) -> Result<()> {

        response.value = lock!(state <= self.state.lock().await?, {
            let mut out = StatusResponse::default();
            out.set_config(state.config.clone());

            // TODO: Simulate slow convergence based on time since last_configured.
            if state.config.has_follower() {
                let proto = out.follower_mut();
                proto.set_got_sync(true);
                proto.set_last_leader_error(0.001);
                proto.set_last_leader_rtt(0.001);
                proto.set_last_sync_age(0.1);
            }

            out
        });

        Ok(())
    }

    async fn Configure(
        &self,
        request: rpc::ServerRequest<ConfigureRequest>,
        response: &mut rpc::ServerResponse<ConfigureResponse>
    ) -> Result<()> {
        lock!(state <= self.state.lock().await?, {
            state.config = request.config().clone();
            state.last_configured = Instant::now();
        });
        Ok(())
    }

    async fn Sync(
        &self,
        request: rpc::ServerRequest<TimeSyncPoint>,
        response: &mut rpc::ServerResponse<SyncResponse>
    ) -> Result<()> {
        Ok(())
    }
}
