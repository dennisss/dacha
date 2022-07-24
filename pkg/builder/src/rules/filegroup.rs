use common::errors::*;

use crate::proto::config::BuildConfig;
use crate::proto::rule::*;
use crate::rule::*;
use crate::target::*;

pub struct FileGroup {
    attrs: FileGroupAttrs,
}

impl BuildRule for FileGroup {
    type Attributes = FileGroupAttrs;

    type Target = Self;

    fn evaluate(attributes: Self::Attributes, config: &BuildConfig) -> Result<Self::Target> {
        Ok(Self { attrs: attributes })
    }
}

#[async_trait]
impl BuildTarget for FileGroup {
    fn name(&self) -> &str {
        self.attrs.name()
    }

    fn dependencies(&self) -> Result<BuildTargetDependencies> {
        // TODO: Depends on all files (they must all by loaded into the environment to
        // be useable).
        Ok(BuildTargetDependencies::default())
    }

    async fn build(&self, context: &BuildTargetContext) -> Result<BuildTargetOutputs> {
        let mut outputs = BuildTargetOutputs::default();

        for src in self.attrs.srcs() {
            let (output_path, source_path) = {
                if let Some(abs_path) = src.strip_prefix("//") {
                    (abs_path.to_string(), context.workspace_dir.join(abs_path))
                } else {
                    (
                        format!("{}/{}", context.key.label.directory, src),
                        context.package_dir.join(src),
                    )
                }
            };

            if !source_path.exists().await {
                return Err(format_err!("Source file does not exist: {}", src));
            }

            outputs.output_files.insert(
                output_path,
                BuildOutputFile {
                    location: source_path,
                },
            );
        }

        Ok(outputs)
    }
}
