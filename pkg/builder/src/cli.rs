use std::path::Path;

use common::errors::*;
use file::{project_dir, project_path};

use crate::builder::Builder;
use crate::context::BuildConfigTarget;
use crate::utils::create_or_update_symlink;
use crate::{LOCAL_BINARY_PATH, NATIVE_CONFIG_LABEL};

#[derive(Args)]
struct Args {
    command: ArgCommand,
}

#[derive(Args)]
enum ArgCommand {
    #[arg(name = "build")]
    Build(BuildCommand),
}

#[derive(Args)]
struct BuildCommand {
    #[arg(positional)]
    label: String,

    #[arg(desc = "Label for a BuildConfig to use for configuring the build.")]
    config: Option<String>,
}

pub fn run() -> Result<()> {
    executor::run(async {
        let args = common::args::parse_args::<Args>()?;
        match args.command {
            ArgCommand::Build(build) => {
                // TODO: Support the --config flag.

                let mut builder = Builder::default()?;

                let result = builder
                    .build_target_cwd(
                        &build.label,
                        build
                            .config
                            .as_ref()
                            .map(|s| s.as_str())
                            .unwrap_or(NATIVE_CONFIG_LABEL),
                    )
                    .await?;

                create_or_update_symlink(
                    format!("built-config/{}", result.config_hash),
                    project_path!("built"),
                )
                .await?;

                let local_bin_dir = project_path!(LOCAL_BINARY_PATH);
                for (src, file) in &result.outputs.output_files {
                    if Path::new(src).starts_with(LOCAL_BINARY_PATH) {
                        create_or_update_symlink(&file.location, project_dir().join(src)).await?;
                    }
                }

                println!("BuildResult:\n{:#?}", result);
            }
        }

        Ok(())
    })?
}
