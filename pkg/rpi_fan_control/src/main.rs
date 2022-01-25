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

*/

extern crate rpi;
#[macro_use]
extern crate common;
extern crate http;
#[macro_use]
extern crate macros;
extern crate rpc_util;
extern crate rpi_fan_control;

use std::ops::Add;
use std::sync::Arc;
use std::time::Duration;

use common::async_std::sync::Mutex;
use common::async_std::task;
use common::bundle::TaskResultBundle;
use common::errors::*;
use google::proto::empty::Empty;
use rpc_util::NamedPortArg;
use rpi::gpio::*;
use rpi::pwm::SysPWM;
use rpi::temp::*;
use rpi_fan_control::proto::rpi_fan_control::*;

const UPDATE_INTERVAL: Duration = Duration::from_millis(250);

const TEMP_FILTER_ALPHA: f32 = 0.9;

const FAN_PWM_FREQUENCY: f32 = 25000.0;

#[derive(Clone)]
struct FanControlServiceImpl {
    state: Arc<Mutex<State>>,
}

struct State {
    proto: FanControlState,
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
            state: Arc::new(Mutex::new(State { proto })),
        })
    }

    pub async fn run(self, pin: GPIOPin) -> Result<()> {
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

                fan_pwm.write(FAN_PWM_FREQUENCY, state.proto.current_speed());

                task::sleep(UPDATE_INTERVAL).await;
            }
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
}

#[derive(Args)]
struct Args {
    rpc_port: NamedPortArg,
    web_port: NamedPortArg,
    fan_pwm_pin: Option<usize>,
}

async fn run() -> Result<()> {
    let args = common::args::parse_args::<Args>()?;

    let mut task_bundle = TaskResultBundle::new();

    let service = FanControlServiceImpl::create()?;

    task_bundle.add("WebServer", {
        let vars = json::Value::Object(map!(
            "rpc_address" => &json::Value::String(format!("http://localhost:{}", args.rpc_port.value()))
        ));

        let web_handler = web::WebServerHandler::new(web::WebServerOptions {
            pages: vec![web::WebPageOptions {
                title: "Fan Control".into(),
                path: "/".into(),
                script_path: "built/pkg/rpi_fan_control/app.js".into(),
                vars: Some(vars),
            }],
        });

        let web_server = http::Server::new(web_handler, http::ServerOptions::default());

        web_server.run(args.web_port.value())
    });

    task_bundle.add("RpcServer", {
        let mut rpc_server = rpc::Http2Server::new();
        rpc_server.add_service(service.clone().into_service())?;
        rpc_server.enable_cors();
        rpc_server.allow_http1();
        rpc_server.run(args.rpc_port.value())
    });

    if let Some(pin) = args.fan_pwm_pin {
        let gpio = rpi::gpio::GPIO::open()?;
        let pin = gpio.pin(pin);
        task_bundle.add("FanControlService::run()", service.run(pin));
    }

    task_bundle.join().await?;

    Ok(())
}

fn main() -> Result<()> {
    task::block_on(run())
}
