use std::sync::Arc;

use common::errors::*;
use crypto::hasher::Hasher;
use crypto::sip::SipHasher;
use protobuf::Message;

use crate::label::Label;
use crate::proto::*;

#[derive(Clone)]
pub struct BuildConfigTarget {
    pub label: Label,
    pub config: Arc<BuildConfig>,
    pub config_key: String,
}

impl BuildConfigTarget {
    pub fn default_for_local_machine() -> Result<Self> {
        let mut config = BuildConfig::default();
        config.set_platform(crate::platform::current_platform()?);

        let mut rust_binary = RustBinaryAttrs::default();
        // rust_binary.set_profile("dev");
        rust_binary.set_compiler(RustCompiler::CARGO);

        // TODO: Instead reference //pkg/builder/config:X
        // ^ Yes, this is missing some stuff as is.

        let target = match (config.platform().architecture(), config.platform().os()) {
            (Architecture::AMD64, Os::LINUX) => "x86_64-unknown-linux-gnu",
            (Architecture::ARM32v7, Os::LINUX) => "armv7-unknown-linux-gnueabihf",
            _ => {
                return Err(err_msg("Unsupported default rust target"));
            }
        };
        rust_binary.set_target(target);

        config.add_rule_defaults({
            let mut any = protobuf_builtins::google::protobuf::Any::default();
            any.pack_from(&rust_binary)?;
            any
        });

        Self::from(Label::parse(crate::NATIVE_CONFIG_LABEL)?, config)
    }

    pub fn from(label: Label, mut config: BuildConfig) -> Result<Self> {
        // Make consistent for keying.
        config.set_name("");

        let config_key = {
            let data = config.serialize()?;
            let mut hasher = SipHasher::default_rounds_with_key_halves(0, 0);
            hasher.update(&data);
            format!("{:016x}", hasher.finish_u64())
        };

        Ok(Self {
            label,
            config: Arc::new(config),
            config_key,
        })
    }
}
