extern crate alloc;
extern crate core;

#[macro_use]
extern crate common;
#[macro_use]
extern crate macros;
extern crate protobuf;
extern crate rpc;
extern crate web;

pub mod cpu_temperature;
pub mod dummy;
pub mod entity;
pub mod fan;
pub mod fan_curve;
pub mod ws2812;

use std::sync::Arc;

use common::errors::*;
use protobuf::{
    message_factory::{MessageFactory, StaticManualMessageFactory},
    text::ParseTextProtoOptions,
};
use rpi_controller_proto::rpi::*;

pub fn entity_message_factory() -> Arc<dyn MessageFactory> {
    let mut factory = StaticManualMessageFactory::default();
    factory
        .add::<DummyEntityConfig>()
        .add::<FanConfig>()
        .add::<FanState>()
        .add::<WS2812StripConfig>()
        .add::<WS2812StripState>()
        .add::<FanCurveConfig>()
        .add::<FanCurveState>()
        .add::<TemperatureSensorConfig>()
        .add::<TemperatureSensorState>();

    Arc::new(factory)
}

pub fn minimal_config() -> Result<ControllerConfig> {
    let mut config = ControllerConfig::default();

    let factory = entity_message_factory();
    let mut options = ParseTextProtoOptions::default();
    options.message_factory = Some(factory.as_ref());

    protobuf::text::parse_text_proto_with_options(
        r#"
        entities {
            id: "cpu_temp"
            value {
                [type.googleapis.com/rpi.TemperatureSensorConfig] {}
            }
        }
        entities {
            id: "fan0"
            value {
                [type.googleapis.com/rpi.DummyEntityConfig] {
                    initial_state {
                        [type.googleapis.com/rpi.FanState] {
                            target_speed: 0.9
                            measured_rpm: 3000
                        }
                    }
                }
            }
        }
        entities {
            id: "strip0"
            value {
                [type.googleapis.com/rpi.DummyEntityConfig] {
                    initial_state {
                        [type.googleapis.com/rpi.WS2812StripState] {
                            periods: []
                        }
                    }
                }
            }
        }
        entities {
            id: "fan_curve0"
            value {
                [type.googleapis.com/rpi.DummyEntityConfig] {
                    initial_state {
                        [type.googleapis.com/rpi.FanCurveState] {
                            enabled: true
                        }
                    }
                }
            }
        }
        "#,
        &mut config,
        &options,
    )?;

    Ok(config)
}

pub fn pi_rack_r5_config() -> Result<ControllerConfig> {
    let mut config = ControllerConfig::default();

    let factory = entity_message_factory();
    let mut options = ParseTextProtoOptions::default();
    options.message_factory = Some(factory.as_ref());

    protobuf::text::parse_text_proto_with_options(
        r#"
        entities {
            id: "cpu_temp"
            value {
                [type.googleapis.com/rpi.TemperatureSensorConfig] {}
            }
        }

        entities {
            id: "fan0"
            value {
                [type.googleapis.com/rpi.FanConfig] {
                    pwm_pin: 18
                    pwm_inverted: true
                    tachometer_pin: 17
                }
            }
        }
        entities {
            id: "strip0"
            value {
                [type.googleapis.com/rpi.WS2812StripConfig] {
                    num_leds: 2
                    serial_pin: 21
                }
            }
        }
        entities {
            id: "fan_curve0"
            value {
                [type.googleapis.com/rpi.FanCurveConfig] {
                    temperature_id: "cpu_temp"
                    fan_ids: ["fan0"]
                    points { temp: 0 speed: 0.3 }
                    points { temp: 30 speed: 0.4 }
                    points { temp: 40 speed: 0.8 }
                    points { temp: 45 speed: 1 }
                }
            }
        }
        "#,
        &mut config,
        &options,
    )?;

    Ok(config)
}
