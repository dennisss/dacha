use std::sync::Arc;
use std::time::Duration;

use common::errors::*;
use executor::sync::AsyncMutex;
use executor::{lock, lock_async};
use protobuf_builtins::google::protobuf::Any;
use protobuf_builtins::ToAnyProto;
use rpi_controller_proto::rpi::*;

use crate::entity::Entity;

pub struct DummyEntity {
    shared: Arc<Shared>,
}

struct Shared {
    config: DummyEntityConfig,
    state: AsyncMutex<Any>,
}

impl DummyEntity {
    pub async fn create(config: &DummyEntityConfig) -> Result<Self> {
        let state = config.initial_state().clone();

        let shared = Arc::new(Shared {
            config: config.clone(),
            state: AsyncMutex::new(state),
        });

        Ok(Self { shared })
    }
}

#[async_trait]
impl Entity for DummyEntity {
    async fn config(&self) -> Result<Any> {
        Ok(self.shared.config.to_any_proto()?)
    }

    async fn state(&self) -> Result<Any> {
        Ok(self.shared.state.lock().await?.read_exclusive().clone())
    }

    async fn update(&self, proposed_state: &Any) -> Result<()> {
        lock!(state <= self.shared.state.lock().await?, {
            *state = proposed_state.clone();
        });

        Ok(())
    }
}
