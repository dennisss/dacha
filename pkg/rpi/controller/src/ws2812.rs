use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use common::errors::*;
use executor::sync::AsyncMutex;
use executor::{lock, lock_async};
use executor_multitask::{impl_resource_passthrough, TaskResource};
use protobuf_builtins::google::protobuf::Any;
use protobuf_builtins::ToAnyProto;
use rpi::gpio::GPIO;
use rpi::ws2812::WS2812Controller;
use rpi_controller_proto::rpi::*;

use crate::entity::Entity;

const SLEEP_TIME: Duration = Duration::from_millis(500);

const EFFECT_REFRESH_RATE: Duration = Duration::from_millis(1000 / 30);

const MAX_NUM_PERIODS: usize = 100;

const MAX_PERIOD_DISTANCE: Duration = Duration::from_secs(60);

pub struct WS2812StripController {
    shared: Arc<Shared>,
    background_thread: TaskResource,
}

struct Shared {
    config: WS2812StripConfig,
    state: AsyncMutex<WS2812StripState>,
}

impl_resource_passthrough!(WS2812StripController, background_thread);

impl WS2812StripController {
    pub async fn create(config: &WS2812StripConfig, gpio: &GPIO) -> Result<Self> {
        let leds = WS2812Controller::create(gpio.pin(config.serial_pin() as usize)).await?;

        let mut state = WS2812StripState::default();
        let shared = Arc::new(Shared {
            config: config.clone(),
            state: AsyncMutex::new(state),
        });

        let background_thread = TaskResource::spawn_interruptable(
            "WS2812StripController",
            Self::run(shared.clone(), leds),
        );

        Ok(Self {
            shared,
            background_thread,
        })
    }

    async fn run(shared: Arc<Shared>, mut leds: WS2812Controller) -> Result<()> {
        // let mut last_aligned_time = None;

        let mut all_black = vec![0u32; shared.config.num_leds() as usize];

        loop {
            // TODO: Use the monotonic clock over multiple adjacent short periods.
            let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis() as u64;
            let now_instant = Instant::now();

            // Get the first period to process.
            let period = lock!(state <= shared.state.lock().await?, {
                while let Some(period) = state.periods().get(0) {
                    let period: WS2812StripPeriod = period.as_ref().clone();

                    // Don't process periods that are far in the future.
                    if period.start_time() >= now + 2 * (SLEEP_TIME.as_millis() as u64) {
                        return None;
                    }

                    state.periods_mut().remove(0);

                    // Skip periods that have already elapsed.
                    if period.end_time() <= now {
                        continue;
                    }

                    return Some(period);
                }

                None
            });

            let period = match period {
                Some(v) => v,
                None => {
                    leds.write(&all_black)?;
                    executor::sleep(SLEEP_TIME).await?;
                    continue;
                }
            };

            // Fast processing of the effect.
            loop {
                let now = now + (Instant::now().duration_since(now_instant).as_millis() as u64);

                if now < period.start_time() {
                    leds.write(&all_black)?;
                } else {
                    if now >= period.end_time() {
                        break;
                    }

                    let mut colors = vec![];
                    colors.extend_from_slice(period.color());
                    colors.resize(shared.config.num_leds() as usize, 0);
                    leds.write(&colors)?;
                }

                executor::sleep(EFFECT_REFRESH_RATE).await?;
            }
        }
    }
}

#[async_trait]
impl Entity for WS2812StripController {
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
            .unpack::<WS2812StripState>()?
            .ok_or_else(|| err_msg("Unsupported led strip state"))?;

        let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis() as u64;

        if proposed_state.periods().len() > MAX_NUM_PERIODS {
            return Err(rpc::Status::invalid_argument("Too many periods to process.").into());
        }

        let mut last_end_time = None;

        for period in proposed_state.periods() {
            if period.start_time() < now + (SLEEP_TIME.as_millis() as u64) {
                return Err(rpc::Status::invalid_argument(
                    "First period is too soon in the future to guarantee timely execution.",
                )
                .into());
            }

            if period.start_time() >= period.end_time() {
                return Err(
                    rpc::Status::invalid_argument("end_time must be after start_time.").into(),
                );
            }

            if period.color().len() > self.shared.config.num_leds() as usize {
                return Err(
                    rpc::Status::invalid_argument("Too many LED colors in period effect.").into(),
                );
            }

            if period.end_time() > now + (MAX_PERIOD_DISTANCE.as_millis() as u64) {
                return Err(
                    rpc::Status::invalid_argument("Period ends too far in the future.").into(),
                );
            }

            if let Some(t) = last_end_time {
                if period.start_time() < t {
                    return Err(rpc::Status::invalid_argument(
                        "Overlapping or non-sorted periods.",
                    )
                    .into());
                }
            }

            last_end_time = Some(period.end_time());
        }

        lock!(state <= self.shared.state.lock().await?, {
            *state = proposed_state.clone();
        });

        Ok(())
    }
}
