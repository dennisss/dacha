use std::path::Path;
use std::process::{Command, Stdio};

use common::errors::*;
use file::LocalPath;

use crate::proto::*;
use crate::rule::BuildRule;
use crate::target::*;
use crate::utils::*;

pub struct RustBinary {
    attrs: RustBinaryAttrs,
}

impl BuildRule for RustBinary {
    type Attributes = RustBinaryAttrs;

    type Target = Self;

    fn evaluate(attributes: Self::Attributes, config: &BuildConfig) -> Result<Self::Target> {
        Ok(Self { attrs: attributes })
    }
}

#[async_trait]
impl BuildTarget for RustBinary {
    fn name(&self) -> &str {
        self.attrs.name()
    }

    fn dependencies(&self) -> Result<BuildTargetDependencies> {
        Ok(BuildTargetDependencies::default())
    }

    async fn build(&self, context: &BuildTargetContext) -> Result<BuildTargetOutputs> {
        // let mut target = context.config.rust_binary().clone();
        // target.merge_from(&raw_target)?;

        let package_name = get_package_name(&context.package_dir).await?;

        let bin_name = if self.attrs.name() == "main" {
            package_name.as_str()
        } else if !self.attrs.bin().is_empty() {
            self.attrs.bin()
        } else {
            self.attrs.name()
        };

        let rust_target_dir = context
            .workspace_dir
            .join("built-rust")
            .join(&context.config_hash);
        // NOTE: we must create the directory otherwise 'cross' tends to screw up the
        // permissions and make root the owner of the directory.
        file::create_dir_all(&rust_target_dir).await?;

        // Add --target-dir when using cross.

        let program = match self.attrs.compiler() {
            RustCompiler::UNKNOWN | RustCompiler::CARGO => "cargo",
            RustCompiler::CROSS => "cross",
        };

        let mut cmd = Command::new(program);

        cmd.arg("build")
            .arg("--package")
            .arg(&package_name)
            .arg("--bin")
            .arg(bin_name)
            .arg("--target-dir")
            .arg(rust_target_dir.as_str())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit());

        let mut effective_profile = "debug";
        match self.attrs.profile() {
            "" => {}
            "release" => {
                cmd.arg("--release");
                effective_profile = "release";
            }
            profile @ _ => return Err(format_err!("Unsupported rust profile: {}", profile)),
        };

        if self.attrs.no_default_features() {
            cmd.arg("--no-default-features");
        }

        // TODO: Make sure that the command doesn't inherit any environment flags not
        // controlled by the build script.
        let mut rust_flags = self.attrs.rustflags().to_string();
        for cfg in self.attrs.cfg() {
            rust_flags.push_str(&format!(" --cfg {}", cfg));
        }
        cmd.env("RUSTFLAGS", rust_flags);

        for var in self.attrs.env() {
            cmd.env(var.key(), var.value());
        }

        // TODO: Assert this is always
        if !self.attrs.target().is_empty() {
            cmd.arg("--target").arg(self.attrs.target());
        }

        let mut child = cmd.spawn()?;

        let status = child.wait()?;
        if !status.success() {
            return Err(format_err!("cargo failed with status: {:?}", status));
        }

        let binary_path = rust_target_dir
            .join(self.attrs.target())
            .join(effective_profile)
            .join(bin_name);

        let mount_path = Path::new("built")
            .join(&context.key.label.directory)
            .join(&context.key.label.target_name)
            .to_str()
            .unwrap()
            .to_string();

        // symlink the 'built-rust' file to the 'built-config' directory so that the
        // symlink that we create from 'built-config' to 'built' works correctly.
        {
            let built_dir_path = context
                .workspace_dir
                .join("built-config")
                .join(&context.config_hash)
                .join(&context.key.label.directory)
                .join(&context.key.label.target_name);

            create_or_update_symlink(&binary_path, &built_dir_path).await?;
        }

        let mut outputs = BuildTargetOutputs::default();

        outputs.output_files.insert(
            mount_path,
            BuildOutputFile {
                location: binary_path,
            },
        );

        Ok(outputs)
    }
}

async fn get_package_name(package_dir: &LocalPath) -> Result<String> {
    let cargo_toml = file::read_to_string(package_dir.join("Cargo.toml")).await?;

    let mut in_package_section = false;

    for line in cargo_toml.lines() {
        if line.starts_with("[") {
            in_package_section = line == "[package]";
        }

        if in_package_section {
            if let Some(name) = line
                .strip_prefix("name = \"")
                .and_then(|s| s.strip_suffix("\""))
            {
                return Ok(name.to_string());
            }
        }
    }

    Err(err_msg("Failed to find package name in Cargo.toml file"))
}
