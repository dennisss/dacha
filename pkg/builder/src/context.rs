use common::errors::*;
use crypto::hasher::Hasher;
use crypto::sip::SipHasher;
use protobuf::Message;

use crate::proto::bundle::*;
use crate::proto::config::*;

pub struct BuildContext {
    pub config: BuildConfig,
    pub config_key: String,
}

impl BuildContext {
    pub async fn default_for_local_machine() -> Result<Self> {
        let mut config = BuildConfig::default();
        config.set_platform(crate::platform::current_platform()?);

        config.rust_binary_mut().set_profile("dev");
        config.rust_binary_mut().set_compiler(RustCompiler::CARGO);

        let target = match (config.platform().architecture(), config.platform().os()) {
            (Architecture::AMD64, Os::LINUX) => "x86_64-unknown-linux-gnu",
            (Architecture::ARM32v7, Os::LINUX) => "armv7-unknown-linux-gnueabihf",
            _ => {
                return Err(err_msg("Unsupported default rust target"));
            }
        };
        config.rust_binary_mut().set_target(target);

        Self::from(config)
    }

    pub fn from(mut config: BuildConfig) -> Result<Self> {
        // Make consistent for keying.
        config.set_name("");

        let config_key = {
            let data = config.serialize()?;
            let mut hasher = SipHasher::default_rounds_with_key_halves(0, 0);
            hasher.update(&data);
            format!("{:016x}", hasher.finish_u64())
        };

        Ok(Self { config, config_key })
    }
}
