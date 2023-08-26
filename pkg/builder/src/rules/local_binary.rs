use common::errors::*;
use file::LocalPath;

use crate::label::Label;
use crate::proto::*;
use crate::rule::*;
use crate::target::*;

pub struct LocalBinary {
    attrs: LocalBinaryAttrs,
}

impl BuildRule for LocalBinary {
    type Attributes = LocalBinaryAttrs;

    type Target = Self;

    fn evaluate(attributes: Self::Attributes, config: &BuildConfig) -> Result<Self::Target> {
        Ok(Self { attrs: attributes })
    }
}

#[async_trait]
impl BuildTarget for LocalBinary {
    fn name(&self) -> &str {
        self.attrs.name()
    }

    fn dependencies(&self) -> Result<BuildTargetDependencies> {
        let mut deps = BuildTargetDependencies::default();

        for label in self.attrs.deps() {
            deps.deps.insert(BuildTargetKey {
                label: Label::parse(label)?,

                // TODO: Have a utility for telling which profile corresponds to the current
                // machine.
                config_label: Label::parse("//pkg/builder/config:x64")?,
            });
        }

        Ok(deps)
    }

    async fn build(&self, context: &BuildTargetContext) -> Result<BuildTargetOutputs> {
        let mut outputs = BuildTargetOutputs::default();

        for (_, dep) in &context.inputs {
            for (src, file) in &dep.output_files {
                let bin_name = LocalPath::new(src.as_str())
                    .file_name()
                    .ok_or_else(|| err_msg("Could not resolve file name for local binary"))?;

                // TODO: Validate that two rules never output to the same path.
                outputs.output_files.insert(
                    format!("{}/{}", crate::LOCAL_BINARY_PATH, bin_name),
                    file.clone(),
                );
            }
        }

        Ok(outputs)
    }
}
