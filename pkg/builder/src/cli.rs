use common::errors::*;

use crate::builder::Builder;
use crate::context::BuildContext;

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

                println!("BuildResult:\n{:#?}", result);
            }
        }

        Ok(())
    })
}
