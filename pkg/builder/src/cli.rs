use common::errors::*;

use crate::context::BuildContext;
use crate::run_build;

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

                run_build(
                    &build.label,
                    &BuildContext::default_for_local_machine().await?,
                )
                .await?;
            }
        }

        Ok(())
    })
}
