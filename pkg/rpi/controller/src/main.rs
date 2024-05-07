/*

Local testing:
    Build the javascript parts:
        cargo run --bin builder -- build //pkg/rpi/controller:app

    Run the server with dummy data:
        cargo run --bin rpi_controller -- --config_name=minimal --rpc_port=8001 --web_port=8000

One off RPI testing:

    cargo run --bin rpi_controller -- --config_name=rpi-rack-r5 --rpc_port=8001 --web_port=8000

    cargo run --bin builder -- build //pkg/rpi/controller:bundle

    scp -i ~/.ssh/id_cluster built/pkg/rpi/controller/bundle/sha256:04ae3b7573b1bd3bb54a2368b4dcd2980fe1ab1986a746156bd3b6a73a371f4a cluster-user@10.1.0.120:~/rpi_fan_control.tar

    ssh -i ~/.ssh/id_cluster cluster-user@10.1.0.120

    rm -r dacha
    mkdir dacha
    tar -xf rpi_fan_control.tar -C dacha
    cd dacha

    ./built/pkg/rpi/controller/rpi_controller --config_name=rpi-rack-r5 --rpc_port=8001 --web_port=8000

*/

extern crate rpi;
#[macro_use]
extern crate common;
extern crate http;
#[macro_use]
extern crate macros;
extern crate rpc_util;
extern crate rpi_controller;

use std::collections::HashMap;
use std::ops::Add;
use std::sync::Arc;
use std::time::Duration;

use common::errors::*;
use executor::bundle::TaskResultBundle;
use executor::sync::AsyncMutex;
use executor::{lock, lock_async};
use executor_multitask::{impl_resource_passthrough, ServiceResource, ServiceResourceGroup};
use protobuf::MessagePtr;
use protobuf_builtins::google::protobuf::Empty;
use rpc_util::NamedPortArg;
use rpi::fan::FanTachometerReader;
use rpi::gpio::*;
use rpi::pwm::SysPWM;
use rpi::temp::*;
use rpi_controller::cpu_temperature::CPUTemperatureController;
use rpi_controller::dummy::DummyEntity;
use rpi_controller::entity::Entity;
use rpi_controller::fan_curve::FanCurveController;
use rpi_controller::ws2812::WS2812StripController;
use rpi_controller::{entity_message_factory, fan::*};
use rpi_controller_proto::rpi::*;

const UPDATE_INTERVAL: Duration = Duration::from_millis(250);

struct Lazy<T, F> {
    value: Option<T>,
    ctor: Option<F>,
}

impl<T, F: FnOnce() -> Result<T>> Lazy<T, F> {
    pub fn new(f: F) -> Self {
        Self {
            value: None,
            ctor: Some(f),
        }
    }

    pub fn get(&mut self) -> Result<&T> {
        if self.value.is_none() {
            let ctor = self.ctor.take().unwrap();
            self.value = Some(ctor()?);
        }

        Ok(self.value.as_ref().unwrap())
    }
}

struct ControllerServiceImpl {
    resource_group: ServiceResourceGroup,
    entities: HashMap<String, Arc<dyn Entity>>,
    config: ControllerConfig,
}

impl_resource_passthrough!(ControllerServiceImpl, resource_group);

impl ControllerServiceImpl {
    pub async fn create(config: &ControllerConfig) -> Result<Self> {
        let mut gpio = Lazy::new(|| GPIO::open());

        let mut entities: HashMap<String, Arc<dyn Entity>> = HashMap::new();
        let resource_group = ServiceResourceGroup::new("Controller");

        for entry in config.entities() {
            let entity: Arc<dyn Entity> = {
                if let Some(config) = entry.value().unpack::<DummyEntityConfig>()? {
                    let entity = Arc::new(DummyEntity::create(&config).await?);
                    entity
                } else if let Some(config) = entry.value().unpack::<TemperatureSensorConfig>()? {
                    let cpu_temp = Arc::new(CPUTemperatureController::create().await?);
                    resource_group.register_dependency(cpu_temp.clone()).await;
                    cpu_temp
                } else if let Some(config) = entry.value().unpack::<FanConfig>()? {
                    let fan = Arc::new(FanController::create(&config, gpio.get()?).await?);
                    resource_group.register_dependency(fan.clone()).await;
                    fan
                } else if let Some(config) = entry.value().unpack::<WS2812StripConfig>()? {
                    let strip =
                        Arc::new(WS2812StripController::create(&config, gpio.get()?).await?);
                    resource_group.register_dependency(strip.clone()).await;
                    strip
                } else if let Some(config) = entry.value().unpack::<FanCurveConfig>()? {
                    let mut fans = vec![];
                    for id in config.fan_ids() {
                        fans.push(
                            entities
                                .get(id)
                                .ok_or_else(|| format_err!("Missing fan with id: {}", id))?
                                .clone(),
                        );
                    }

                    let temp_sensor = entities
                        .get(config.temperature_id())
                        .ok_or_else(|| err_msg("Can't find temperature sensor for fan curve"))?
                        .clone();

                    let curve =
                        Arc::new(FanCurveController::create(&config, temp_sensor, fans).await?);
                    resource_group.register_dependency(curve.clone()).await;
                    curve
                } else {
                    return Err(err_msg("Unsupported entity type"));
                }
            };

            entities.insert(entry.id().to_string(), entity);
        }

        Ok(Self {
            resource_group,
            entities,
            config: config.clone(),
        })
    }
}

#[async_trait]
impl ControllerService for ControllerServiceImpl {
    async fn Read(
        &self,
        request: rpc::ServerRequest<Empty>,
        response: &mut rpc::ServerResponse<ControllerProto>,
    ) -> Result<()> {
        let mut proto = ControllerProto::default();

        for (id, entity) in &self.entities {
            let mut c = proto.config_mut().new_entities();
            c.set_id(id.clone());
            c.set_value(entity.config().await?);

            let mut proto = proto.state_mut().new_entities();
            proto.set_id(id.clone());
            proto.set_value(entity.state().await?);
        }

        response.value = proto;
        Ok(())
    }

    async fn Write(
        &self,
        request: rpc::ServerRequest<ControllerState>,
        response: &mut rpc::ServerResponse<Empty>,
    ) -> Result<()> {
        for e in request.entities() {
            let entity = self.entities.get(e.id()).ok_or_else(|| {
                rpc::Status::invalid_argument(format!("Unknown entity with id: {}", e.id()))
            })?;

            entity.update(e.value()).await?;
        }

        Ok(())
    }
}

#[derive(Args)]
struct Args {
    /// Port on which to start the RPC server.
    rpc_port: NamedPortArg,

    /// Port on which to start the web server.
    web_port: NamedPortArg,

    config_name: String,
}

#[executor_main]
async fn main() -> Result<()> {
    println!("Starting rpi controller");

    let args = common::args::parse_args::<Args>()?;

    let root_resource = executor_multitask::RootResource::new();

    let config = match args.config_name.as_str() {
        "minimal" => rpi_controller::minimal_config()?,
        "rpi_rack_r5" => rpi_controller::pi_rack_r5_config()?,
        _ => return Err(format_err!("Unkown config named: {}", args.config_name)),
    };

    let service = Arc::new(ControllerServiceImpl::create(&config).await?);

    // TODO: Make this a dependency of the rpc server.
    root_resource.register_dependency(service.clone()).await;

    let message_factory = entity_message_factory();

    root_resource
        .register_dependency({
            let vars = json::Value::Object(map!(
                "rpc_port" => &json::Value::String(args.rpc_port.value().to_string())
            ));

            let web_handler = web::WebServerHandler::new(web::WebServerOptions {
                pages: vec![web::WebPageOptions {
                    title: "Fan Control".into(),
                    path: "/".into(),
                    script_path: "built/pkg/rpi/controller/app.js".into(),
                    vars: Some(vars),
                }],
            });

            let mut options = http::ServerOptions::default();
            options.name = "WebServer".to_string();
            options.port = Some(args.web_port.value());

            let web_server = http::Server::new(web_handler, options);
            Arc::new(web_server.start())
        })
        .await;

    root_resource
        .register_dependency({
            let mut rpc_server = rpc::Http2Server::new(Some(args.rpc_port.value()));
            rpc_server.add_service(service.clone().into_service())?;
            rpc_server.enable_cors();
            rpc_server.allow_http1();
            rpc_server.codec_options_mut().json_parser.message_factory =
                Some(message_factory.clone());
            rpc_server
                .codec_options_mut()
                .json_serializer
                .message_factory = Some(message_factory.clone());
            rpc_server.start()
        })
        .await;

    root_resource.wait().await
}
