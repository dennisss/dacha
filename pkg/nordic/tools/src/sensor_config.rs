use alloc::string::String;
use std::collections::HashMap;

use file::project_path;
use common::errors::*;
use nordic_proto::nordic::*;

pub struct SensorConfigRegistry {
    configs: HashMap<String, SensorConfig>
}

impl SensorConfigRegistry {
    pub async fn defaults() -> Result<Self> {

        let mut known_pins = HashMap::new();
        // Port 0 has 32 pins.
        // Port 1 has 16 pins.
        for i in 0..(32 + 16) {
            let port_num = i / 32;
            let port_pin = i % 32;

            let name = format!("P{}.{:02}", port_num, port_pin);
            let index = i as u32;
            known_pins.insert(name, index);
        }


        let mut configs = HashMap::new();

        let dir = project_path!("pkg/nordic/config/sensors");
        for entry in file::read_dir(&dir)? {
            // TODO: Switch to a glob
            if !entry.name().ends_with(".txtpb") {
                continue;
            }

            let path = dir.join(entry.name());
            let config_name = path.file_stem().unwrap().to_string();

            let data: String = file::read_to_string(&path).await?;

            let mut text_proto = protobuf::text::parse_text_syntax(&data)?;

            let mut error = Ok(());
            text_proto.iter_mut(&mut |v| {

                if let protobuf::text::TextValue::String(s) = v {
                    let s = std::str::from_utf8(s).unwrap();

                    if let Some(i) = known_pins.get(s).cloned() {
                        *v = protobuf::text::TextValue::UnsignedInteger(i as u64);
                    } else {
                        error = Err(format_err!("Unknown pin: {}", s));
                    }
                }
            });

            error?;


            let mut config = SensorConfig::default();
            text_proto.apply(&mut config, &protobuf::text::ParseTextProtoOptions::default())
                .map_err(|e| format_err!("While trying to load: {}; {}", entry.name(), e))?;

            configs.insert(config_name, config);
        }

        Ok(Self {
            configs
        })
    }

    pub fn get(&self, name: &str) -> Option<&SensorConfig> {
        self.configs.get(name)
    }
}
