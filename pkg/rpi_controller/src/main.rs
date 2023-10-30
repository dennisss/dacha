/*
/sys/class/pwm/pwmchip0
/sys/class/thermal/thermal_zone0/temp

State:
- Temp reader
- PWM handle
- Whether or not we are currently doing auto-testing or

Fan curve:
- 0C : 20% min
- 50C: 30%
- 60C: 50%
- 70C: 70%
- 80C: 100%

Testing using an independent Pi:

cargo run --bin builder -- build //pkg/rpi_controller:bundle



ssh -i ~/.ssh/id_cluster pi@10.1.0.114

scp -i ~/.ssh/id_cluster built/pkg/rpi_controller/bundle/sha256:1ace733a525dd7e9567ae74174a276eee778888dfd5ae87870dd87355c259916 pi@10.1.0.114:~/rpi_fan_control.tar

rm -r dacha

mkdir dacha

tar -xf rpi_fan_control.tar -C dacha

cd dacha

./built/pkg/rpi_controller/rpi_controller --rpc_port=8001 --web_port=8000

--fan_pwm_pin=18 --fan_inverted

*/

extern crate rpi;
#[macro_use]
extern crate common;
extern crate http;
#[macro_use]
extern crate macros;
extern crate rpc_util;
extern crate rpi_controller;

use std::ops::Add;
use std::sync::Arc;
use std::time::Duration;

use common::errors::*;
use executor::bundle::TaskResultBundle;
use executor::sync::Mutex;
use protobuf_builtins::google::protobuf::Empty;
use rpc_util::NamedPortArg;
use rpi::gpio::*;
use rpi::pwm::SysPWM;
use rpi::temp::*;
use rpi_controller::proto::rpi_fan_control::*;

const UPDATE_INTERVAL: Duration = Duration::from_millis(250);

const TEMP_FILTER_ALPHA: f32 = 0.9;

const FAN_PWM_FREQUENCY: f32 = 25000.0;

#[derive(Clone)]
struct FanControlServiceImpl {
    state: Arc<Mutex<State>>,
}

struct State {
    proto: FanControlState,
    led_pin: Option<GPIOPin>,
}

impl FanControlServiceImpl {
    pub fn create() -> Result<Self> {
        let mut proto = FanControlState::default();
        proto.set_auto(true);
        proto.set_current_speed(0.0);
        proto.set_current_temp(0.0);

        protobuf::text::parse_text_proto(
            "
            points { temp: 0 speed: 0.2 }
            points { temp: 50 speed: 0.3 }
            points { temp: 60 speed: 0.5 }
            points { temp: 70 speed: 0.7 }
            points { temp: 80 speed: 1 }
        ",
            proto.fan_curve_mut(),
        )?;

        Ok(Self {
            state: Arc::new(Mutex::new(State {
                proto,
                led_pin: None,
            })),
        })
    }

    pub async fn run(self, pin: GPIOPin, inverted: bool) -> Result<()> {
        let mut last_temperature = None;

        let mut temp_reader = CPUTemperatureReader::create().await?;
        let mut fan_pwm = SysPWM::open(pin).await?;

        loop {
            {
                let mut state_guard = self.state.lock().await;
                let state = &mut *state_guard;

                let mut temp = temp_reader.read().await? as f32;
                if let Some(last_temp) = last_temperature.take() {
                    temp = temp * TEMP_FILTER_ALPHA + (last_temp * (1.0 - TEMP_FILTER_ALPHA));
                }
                last_temperature = Some(temp);

                state.proto.set_current_temp(temp);

                if state.proto.auto() {
                    state.proto.set_current_speed(Self::select_speed(
                        temp,
                        state.proto.fan_curve().points(),
                    ));
                }

                let duty_cycle = if inverted {
                    1.0 - state.proto.current_speed()
                } else {
                    state.proto.current_speed()
                };

                fan_pwm.write(FAN_PWM_FREQUENCY, duty_cycle).await;
            }

            executor::sleep(UPDATE_INTERVAL).await;
        }
    }

    /// NOTE: We assume that the curve points are sorted by temperature.
    fn select_speed(temp: f32, points: &[FanCurvePoint]) -> f32 {
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

    async fn identify(&self) -> Result<()> {
        let mut led_pin = self
            .state
            .lock()
            .await
            .led_pin
            .clone()
            .ok_or_else(|| rpc::Status::invalid_argument(format!("No LED pin configured.")))?;

        for i in 0..4 {
            led_pin.write(true);
            executor::sleep(Duration::from_millis(250)).await;
            led_pin.write(false);
            executor::sleep(Duration::from_millis(250)).await;
        }

        Ok(())
    }
}

#[async_trait]
impl FanControlService for FanControlServiceImpl {
    async fn Read(
        &self,
        request: rpc::ServerRequest<Empty>,
        response: &mut rpc::ServerResponse<FanControlState>,
    ) -> Result<()> {
        let mut state = self.state.lock().await;
        response.value = state.proto.clone();
        Ok(())
    }

    async fn Write(
        &self,
        request: rpc::ServerRequest<FanControlState>,
        response: &mut rpc::ServerResponse<Empty>,
    ) -> Result<()> {
        let mut state = self.state.lock().await;
        state.proto.set_auto(request.value.auto());
        state.proto.set_current_speed(request.value.current_speed());
        Ok(())
    }

    async fn Identify(
        &self,
        request: rpc::ServerRequest<Empty>,
        response: &mut rpc::ServerResponse<Empty>,
    ) -> Result<()> {
        self.identify().await
    }
}

#[derive(Args)]
struct Args {
    /// Port on which to start the RPC server.
    rpc_port: NamedPortArg,

    /// Port on which to start the web server.
    web_port: NamedPortArg,

    /// Raspberry Pi BCM pin number on which the fan's PWM is connected.
    fan_pwm_pin: Option<usize>,

    /// Whether or not the PWM signal should be inverted (100% duty cycle
    /// actually means 0% fan speed).
    #[arg(default = false)]
    fan_inverted: bool,

    /// Raspberry Pi BCM pin number of a binary on/off LED controlled with
    /// high/low pulses.
    led_pin: Option<usize>,
}

#[executor_main]
async fn main() -> Result<()> {
    println!("Starting rpi controller");

    let args = common::args::parse_args::<Args>()?;

    let mut task_bundle = TaskResultBundle::new();

    let service = FanControlServiceImpl::create()?;

    task_bundle.add("WebServer", {
        let vars = json::Value::Object(map!(
            "rpc_port" => &json::Value::String(args.rpc_port.value().to_string())
        ));

        let web_handler = web::WebServerHandler::new(web::WebServerOptions {
            pages: vec![web::WebPageOptions {
                title: "Fan Control".into(),
                path: "/".into(),
                script_path: "built/pkg/rpi_controller/app.js".into(),
                vars: Some(vars),
            }],
        });

        let web_server = http::Server::new(web_handler, http::ServerOptions::default());
        // TODO: Add a shutdown token.

        web_server.run(args.web_port.value())
    });

    task_bundle.add("RpcServer", {
        let mut rpc_server = rpc::Http2Server::new();
        rpc_server.set_shutdown_token(executor::signals::new_shutdown_token());
        rpc_server.add_service(service.clone().into_service())?;
        rpc_server.enable_cors();
        rpc_server.allow_http1();
        rpc_server.run(args.rpc_port.value())
    });

    if args.fan_pwm_pin.is_some() || args.led_pin.is_some() {
        let gpio = rpi::gpio::GPIO::open()?;

        if let Some(pin) = args.led_pin {
            let mut pin = gpio.pin(pin);
            pin.set_mode(rpi::gpio::Mode::Output);
            pin.write(false);
            service.state.lock().await.led_pin = Some(pin);
        }

        if let Some(pin) = args.fan_pwm_pin {
            let pin = gpio.pin(pin);
            task_bundle.add(
                "FanControlService::run()",
                service.run(pin, args.fan_inverted),
            );
        }
    }

    task_bundle.join().await?;

    Ok(())
}
