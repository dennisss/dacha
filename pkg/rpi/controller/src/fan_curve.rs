use std::sync::Arc;
use std::time::Duration;

use common::errors::*;
use executor::sync::AsyncMutex;
use executor::{lock, lock_async};
use executor_multitask::{impl_resource_passthrough, TaskResource};
use protobuf::MessagePtr;
use protobuf_builtins::google::protobuf::Any;
use protobuf_builtins::ToAnyProto;
use rpi_controller_proto::rpi::*;

use crate::entity::Entity;

pub struct FanCurveController {
    shared: Arc<Shared>,
    background_thread: TaskResource,
}

struct Shared {
    config: FanCurveConfig,
    state: AsyncMutex<FanCurveState>,
}

impl_resource_passthrough!(FanCurveController, background_thread);

impl FanCurveController {
    pub async fn create(
        config: &FanCurveConfig,
        temp_sensor: Arc<dyn Entity>,
        fans: Vec<Arc<dyn Entity>>,
    ) -> Result<Self> {
        let mut state = FanCurveState::default();
        state.set_enabled(true);

        let shared = Arc::new(Shared {
            config: config.clone(),
            state: AsyncMutex::new(state),
        });

        let background_thread = TaskResource::spawn_interruptable(
            "FanCurveController",
            Self::run(shared.clone(), temp_sensor, fans),
        );

        Ok(Self {
            shared,
            background_thread,
        })
    }

    async fn run(
        shared: Arc<Shared>,
        temp_sensor: Arc<dyn Entity>,
        fans: Vec<Arc<dyn Entity>>,
    ) -> Result<()> {
        loop {
            let enabled = shared.state.lock().await?.read_exclusive().enabled();
            if enabled {
                let temp_state = temp_sensor
                    .state()
                    .await?
                    .unpack::<TemperatureSensorState>()?
                    .ok_or_else(|| err_msg("Wrong type received from temperature sensor"))?;

                let speed = Self::select_speed(temp_state.temperature(), shared.config.points());

                let mut fan_state = FanState::default();
                fan_state.set_target_speed(speed);

                let fan_state = fan_state.to_any_proto()?;

                for fan in &fans {
                    fan.update(&fan_state).await?;
                }
            }

            executor::sleep(Duration::from_secs(1)).await?;
        }
    }

    /// NOTE: We assume that the curve points are sorted by temperature.
    fn select_speed(temp: f32, points: &[MessagePtr<FanCurvePoint>]) -> f32 {
        if points.len() <= 1 {
            return 1.0;
        }

        if temp < points[0].temp() {
            return 0.0;
        } else if temp > points[points.len() - 1].temp() {
            return 1.0;
        }

        for i in 0..(points.len() - 1) {
            let start = &points[i];
            let end = &points[i + 1];

            if temp >= start.temp() && temp <= end.temp() {
                let v = (temp - start.temp()) / (end.temp() - start.temp());
                let speed = v * (end.speed() - start.speed()) + start.speed();
                return speed;
            }
        }

        1.0
    }
}

#[async_trait]
impl Entity for FanCurveController {
    async fn config(&self) -> Result<Any> {
        Ok(self.shared.config.to_any_proto()?)
    }

    async fn state(&self) -> Result<Any> {
        Ok(self
            .shared
            .state
            .lock()
            .await?
            .read_exclusive()
            .to_any_proto()?)
    }

    async fn update(&self, proposed_state: &Any) -> Result<()> {
        let proposed_state = proposed_state
            .unpack::<FanCurveState>()?
            .ok_or_else(|| rpc::Status::invalid_argument("Unsupported fan state type"))?;

        lock!(state <= self.shared.state.lock().await?, {
            *state = proposed_state;
        });

        Ok(())
    }
}
