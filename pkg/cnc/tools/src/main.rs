#[macro_use]
extern crate macros;

use std::time::{Duration, Instant};
use std::collections::HashMap;

use base_args::define_arg_command;
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

define_arg_command!(Mode {
    ExecuteCommand = "execute",
    SkewCalibrationCommand = "skew-calibration",
    LevelingCommand = "leveling",
    BenchmarkCommand = "benchmark",
    MotionAnalysisCommand = "motion-analysis"
});

#[executor_main]
async fn main() -> Result<()> {
    let args = base_args::parse_args::<Args>()?;

    args.mode.run().await?;

    Ok(())
}