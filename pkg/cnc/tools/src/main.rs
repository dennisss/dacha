#[macro_use]
extern crate macros;

use std::time::{Duration, Instant};
use std::collections::HashMap;

use common::errors::*;
use file::{LocalPath, LocalPathBuf, LocalFile};
use cnc_tools::execute::*;
use cnc_tools::skew::*;
use cnc_tools::leveling::*;


#[derive(Args)]
struct Args {
    mode: Mode,
}

#[derive(Args)]
enum Mode {
    #[arg(name = "execute")]
    Execute(ExecuteCommand),

    #[arg(name = "skew-calibration")]
    SkewCalibration(SkewCalibrationCommand),

    #[arg(name = "leveling")]
    Leveling(LevelingCommand)
}

impl Mode {
    async fn run(self) -> Result<()> {
        match self {
            Self::Execute(cmd) => cmd.run().await,
            Self::SkewCalibration(cmd) => cmd.run().await,
            Self::Leveling(cmd) => cmd.run().await,
        }
    }
}

#[executor_main]
async fn main() -> Result<()> {
    let args = base_args::parse_args::<Args>()?;

    args.mode.run().await?;

    Ok(())
}