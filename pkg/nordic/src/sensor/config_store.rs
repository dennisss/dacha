use common::errors::*;
use common::list::Appendable;
use common::segmented_buffer::SegmentedBuffer;
use crypto::ccm::CCM;
use executor::channel::Channel;
use executor::futures::*;
use executor::lock_async;
use executor::sync::{AsyncMutex, AsyncMutexGuard, AsyncMutexReadOnlyGuard};
use nordic_proto::nordic::{SensorConfig};

use crate::params::{AppParamsStorage, SENSOR_CONFIG_ID};


pub struct SensorConfigStore {
    state: AsyncMutex<State>,
    params_storage: &'static AppParamsStorage,
}

struct State {
    config: SensorConfig
}

impl SensorConfigStore {

    pub async fn create(params_storage: &'static AppParamsStorage) -> Result<Self> {
        let mut config = SensorConfig::default();
        let found_config = params_storage
            .read_into_proto(SENSOR_CONFIG_ID, &mut config)
            .await?;

        Ok(Self {
            state: AsyncMutex::new(State {
                config
            }),
            params_storage,
        })
    }

    pub async fn set_config(&self, config: SensorConfig) -> Result<()> {
        lock_async!(state <= self.state.lock().await?, {
            state.config = config;

            self.params_storage
                .write_proto(SENSOR_CONFIG_ID, &state.config)
                .await?;

            Ok(())
        })
    }

    pub async fn get_config(&self) -> Result<SensorConfig> {
        lock_async!(state <= self.state.lock().await?, {
            Ok(state.config.clone())
        })
    }

}
