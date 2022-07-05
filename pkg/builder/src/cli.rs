use std::path::Path;

use common::{errors::*, project_dir, project_path};

use crate::builder::Builder;
use crate::context::BuildContext;
use crate::LOCAL_BINARY_PATH;

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
    common::async_std::task::block_on(async {
        let args = common::args::parse_args::<Args>()?;
        match args.command {
            ArgCommand::Build(build) => {
                // TODO: Support the --config flag.

                let mut builder = Builder::default();

                let build_context = match build.config {
                    Some(label) => BuildContext::from(builder.lookup_config(&label, None).await?)?,
                    None => BuildContext::default_for_local_machine().await?,
                };

                let result = builder
                    .build_target_cwd(&build.label, &build_context)
                    .await?;

                let built_link_out = project_path!("built");
                if let Ok(_) = built_link_out.symlink_metadata() {
                    std::fs::remove_file(&built_link_out)?;
                }

                std::os::unix::fs::symlink(
                    format!("built-config/{}", result.key.config_key),
                    built_link_out,
                )?;

                let local_bin_dir = project_path!(LOCAL_BINARY_PATH);
                if !local_bin_dir.exists() {
                    std::fs::create_dir(&local_bin_dir)?;
                }

                for (src, path) in &result.output_files {
                    if Path::new(src).starts_with(LOCAL_BINARY_PATH) {
                        let output_path = project_dir().join(src);
                        if let Ok(_) = output_path.symlink_metadata() {
                            std::fs::remove_file(&output_path)?;
                        }

                        std::os::unix::fs::symlink(path, output_path)?;
                    }
                }

                println!("BuildResult:\n{:#?}", result);
            }
        }

        Ok(())
    })
}
