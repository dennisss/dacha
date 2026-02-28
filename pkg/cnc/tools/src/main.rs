#[macro_use]
extern crate macros;

use std::time::{Duration, Instant};
use std::collections::HashMap;

use common::errors::*;
use file::{LocalPath, LocalPathBuf, LocalFile};
use cnc_tools::execute::*;
use cnc_tools::skew::*;
use cnc_tools::leveling::*;
use cnc_tools::benchmark::*;
use cnc_tools::motion_analysis::*;

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
    Leveling(LevelingCommand),

    #[arg(name = "benchmark")]
    Benchmark(BenchmarkCommand),

    #[arg(name = "motion-analysis")]
    MotionAnalysis(MotionAnalysisCommand)
}

impl Mode {
    async fn run(self) -> Result<()> {
        match self {
            Self::Execute(cmd) => cmd.run().await,
            Self::SkewCalibration(cmd) => cmd.run().await,
            Self::Leveling(cmd) => cmd.run().await,
            Self::Benchmark(cmd) => cmd.run().await,
            Self::MotionAnalysis(cmd) => cmd.run().await,
        }
    }
}

#[executor_main]
async fn main() -> Result<()> {
    let args = base_args::parse_args::<Args>()?;

    args.mode.run().await?;

    Ok(())
}