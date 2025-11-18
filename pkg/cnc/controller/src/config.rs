use std::collections::HashMap;

use common::errors::*;
use cnc_controller_proto::cnc::*;
use file::project_path;


pub struct ControllerConfigRegistry {
    configs: HashMap<String, ControllerConfig>,
}

impl ControllerConfigRegistry {
    // TODO: Dedup this logic with the other config registries we have.
    pub async fn defaults() -> Result<Self> {
        let mut configs = HashMap::new();

        let dir = project_path!("pkg/cnc/controller/config");
        for entry in file::read_dir(&dir)? {
            // TODO: Switch to a glob
            if !entry.name().ends_with(".txtpb") {
                continue;
            }

            let data: String = file::read_to_string(&dir.join(entry.name())).await?;

            let mut config = ControllerConfig::default();
            protobuf::text::parse_text_proto(&data, &mut config)
                .map_err(|e| format_err!("While trying to load: {}; {}", entry.name(), e))?;

            if configs.insert(config.name().to_string(), config).is_some() {
                return Err(err_msg("Duplicate config"));
            }
        }

        Ok(Self {
            configs
        })
    }

    pub fn remove(&mut self, name: &str) -> Option<ControllerConfig> {
        self.configs.remove(name)
    }
}