use std::sync::Arc;
use std::time::Duration;

use common::errors::*;
use executor::sync::AsyncMutex;
use executor::{lock, lock_async};
use executor_multitask::{impl_resource_passthrough, TaskResource};
use protobuf_builtins::google::protobuf::Any;
use protobuf_builtins::ToAnyProto;
use rpi::fan::FanTachometerReader;
use rpi::gpio::*;
use rpi::pwm::SysPWM;
use rpi_controller_proto::rpi::*;

use crate::entity::Entity;

const FAN_PWM_FREQUENCY: f32 = 25000.0;

pub struct FanController {
    shared: Arc<Shared>,
    background_thread: TaskResource,
}

struct Shared {
    config: FanConfig,
    state: AsyncMutex<FanState>,
}

impl_resource_passthrough!(FanController, background_thread);

impl FanController {
    pub async fn create(config: &FanConfig, gpio: &GPIO) -> Result<Self> {
        let pwm = SysPWM::open(gpio.pin(config.pwm_pin() as usize)).await?;
        let tach = FanTachometerReader::create(gpio.pin(config.tachometer_pin() as usize));

        let mut state = FanState::default();
        state.set_target_speed(1.0);

        let shared = Arc::new(Shared {
            config: config.clone(),
            state: AsyncMutex::new(state),
        });

        let background_thread = TaskResource::spawn_interruptable(
            "FanController",
            Self::run(shared.clone(), pwm, tach),
        );

        Ok(Self {
            shared,
            background_thread,
        })
    }

    async fn run(
        shared: Arc<Shared>,
        mut pwm: SysPWM,
        mut tach: FanTachometerReader,
    ) -> Result<()> {
        loop {
            let measured_rpm = tach.read().await?;

            let mut target_speed = 0.0;
            lock!(state <= shared.state.lock().await?, {
                state.set_measured_rpm(measured_rpm as u32);
                target_speed = state.target_speed();
            });

            let mut duty_cycle = target_speed;
            if shared.config.pwm_inverted() {
                duty_cycle = 1.0 - duty_cycle;
            }

            pwm.write(FAN_PWM_FREQUENCY, duty_cycle).await?;

            executor::sleep(Duration::from_secs(1)).await?;
        }
    }
}

#[async_trait]
impl Entity for FanController {
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
            .unpack::<FanState>()?
            .ok_or_else(|| rpc::Status::invalid_argument("Unsupported fan state type"))?;

        if proposed_state.target_speed() < 0.0 || proposed_state.target_speed() > 1.0 {
            return Err(rpc::Status::invalid_argument("Invalid target_speed for fan.").into());
        }

        lock!(state <= self.shared.state.lock().await?, {
            state.set_target_speed(proposed_state.target_speed());
        });

        Ok(())
    }
}
