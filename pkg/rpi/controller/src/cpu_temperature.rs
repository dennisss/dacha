use std::sync::Arc;
use std::time::Duration;

use common::errors::*;
use executor::sync::AsyncMutex;
use executor::{lock, lock_async};
use executor_multitask::{impl_resource_passthrough, TaskResource};
use protobuf_builtins::google::protobuf::Any;
use protobuf_builtins::ToAnyProto;
use rpi::temp::CPUTemperatureReader;
use rpi_controller_proto::rpi::*;

use crate::entity::Entity;

const TEMP_FILTER_ALPHA: f32 = 0.9;

const UPDATE_INTERVAL: Duration = Duration::from_millis(250);

pub struct CPUTemperatureController {
    shared: Arc<Shared>,
    background_thread: TaskResource,
}

struct Shared {
    // TODO: Timestamp all measurements like this.
    state: AsyncMutex<f32>,
}

impl_resource_passthrough!(CPUTemperatureController, background_thread);

impl CPUTemperatureController {
    pub async fn create() -> Result<Self> {
        let mut temp_reader = CPUTemperatureReader::create().await?;

        let shared = Arc::new(Shared {
            // TODO: Properly initialize with a valid value.
            state: AsyncMutex::new(0.0),
        });

        let background_thread = TaskResource::spawn_interruptable(
            "CPUTemperatureController",
            Self::run(shared.clone(), temp_reader),
        );

        Ok(Self {
            shared,
            background_thread,
        })
    }

    async fn run(shared: Arc<Shared>, mut temp_reader: CPUTemperatureReader) -> Result<()> {
        let mut last_temperature = None;

        loop {
            let mut temp = temp_reader.read().await? as f32;
            if let Some(last_temp) = last_temperature.take() {
                temp = temp * TEMP_FILTER_ALPHA + (last_temp * (1.0 - TEMP_FILTER_ALPHA));
            }
            last_temperature = Some(temp);

            lock!(state <= shared.state.lock().await?, {
                *state = temp;
            });

            executor::sleep(UPDATE_INTERVAL).await?;
        }
    }
}

#[async_trait]
impl Entity for CPUTemperatureController {
    async fn config(&self) -> Result<Any> {
        Ok(TemperatureSensorConfig::default().to_any_proto()?)
    }

    async fn state(&self) -> Result<Any> {
        let mut proto = TemperatureSensorState::default();
        proto.set_temperature(*self.shared.state.lock().await?.read_exclusive());

        Ok(proto.to_any_proto()?)
    }

    async fn update(&self, proposed_state: &Any) -> Result<()> {
        Err(
            rpc::Status::invalid_argument("Can not update the state of a temperature sensor")
                .into(),
        )
    }
}
