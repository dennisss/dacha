use std::path::Path;
use std::process::{Command, Stdio};

use common::errors::*;

use crate::label::Label;
use crate::proto::config::BuildConfig;
use crate::proto::rule::*;
use crate::rule::*;
use crate::target::*;

pub struct Webpack {
    attrs: WebpackAttrs,
}

impl BuildRule for Webpack {
    type Attributes = WebpackAttrs;

    type Target = Self;

    fn evaluate(attributes: Self::Attributes, config: &BuildConfig) -> Result<Self::Target> {
        Ok(Self { attrs: attributes })
    }
}

#[async_trait]
impl BuildTarget for Webpack {
    fn name(&self) -> &str {
        self.attrs.name()
    }

    fn dependencies(&self) -> Result<BuildTargetDependencies> {
        let mut deps = BuildTargetDependencies::default();

        // TODO: This has a lot of JavaScript file dependencies.

        /*
        for label in self.attrs.deps() {
            deps.deps.insert(BuildTargetKey {
                label: Label::parse(label)?,

                // TODO: Have a utility for telling which profile corresponds to the current
                // machine.
                config_label: Label::parse("//pkg/builder/config:x64")?,
            });
        }
        */

        Ok(deps)
    }

    async fn build(&self, context: &BuildTargetContext) -> Result<BuildTargetOutputs> {
        // TODO: Verify at most one webpack target is present per build directory.

        let output_mount_path = Path::new("built")
            .join(&context.key.label.directory)
            .join(format!("{}.js", context.key.label.target_name));

        let output_path = context
            .workspace_dir
            .join("built-config")
            .join(&context.config_hash)
            .join(&context.key.label.directory)
            .join(format!("{}.js", context.key.label.target_name));

        let bin = context.workspace_dir.join("node_modules/.bin/webpack");

        let mut child = Command::new(bin)
            .arg("--config")
            .arg(context.workspace_dir.join("pkg/web/webpack.config.js"))
            .arg("--env")
            .arg(&format!(
                "entry={}",
                context
                    .package_dir
                    .join(self.attrs.entry())
                    .to_str()
                    .unwrap()
            ))
            .arg("--env")
            .arg(&format!("output={}", output_path.to_str().unwrap()))
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .spawn()?;

        let status = child.wait()?;
        if !status.success() {
            return Err(format_err!("Webpack failed: {:?}", status));
        }

        let mut outputs = BuildTargetOutputs::default();
        outputs.output_files.insert(
            output_mount_path.to_str().unwrap().to_string(),
            BuildOutputFile {
                location: output_path,
            },
        );

        Ok(outputs)
    }
}
