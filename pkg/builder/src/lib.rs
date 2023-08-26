extern crate alloc;
extern crate core;

#[macro_use]
extern crate common;
#[macro_use]
extern crate macros;
extern crate compression;
extern crate crypto;
#[macro_use]
extern crate file;

mod builder;
pub mod cli;
mod context;
mod label;
mod package;
mod platform;
pub use builder_proto::builder as proto;
pub mod rule;
mod rules;
pub mod target;
mod utils;

pub use self::builder::Builder;
pub use context::BuildConfigTarget;
pub use platform::current_platform;

pub const LOCAL_BINARY_PATH: &'static str = "bin";

/// Label of the rule which produces appropriate settings for the current
/// machine.
pub const NATIVE_CONFIG_LABEL: &'static str = "//pkg/builder/config:native";

#[cfg(test)]
mod tests {
    use common::errors::*;
    use protobuf::{text::ParseTextProto, StaticMessage};

    use crate::proto::bundle::BundleSpec;

    use super::*;

    #[testcase]
    async fn able_to_build_test_bundle() -> Result<()> {
        let tmp_dir = file::temp::TempDir::create()?;

        let workspace = tmp_dir.path().join("workspace");
        file::copy_all(project_path!("testdata/builder/workspace1"), &workspace).await?;

        let mut builder = Builder::new(&workspace)?;
        let outputs = builder
            .build_target("//bundles:asset_bundle", NATIVE_CONFIG_LABEL, None)
            .await?;

        let spec_path = outputs
            .outputs
            .output_files
            .get("built/bundles/asset_bundle/spec.textproto")
            .unwrap()
            .location
            .clone();

        let spec = BundleSpec::parse_text(&file::read_to_string(&spec_path).await?)?;

        assert!(outputs.outputs.output_files.contains_key("built/bundles/asset_bundle/sha256:53c91581a9a18e029113480a54ce9687ff65ba64f52ce23441e80fa08071436b"));

        Ok(())
    }
}
