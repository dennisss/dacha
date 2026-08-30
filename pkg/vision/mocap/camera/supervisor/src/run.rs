use std::process::Command;

use common::errors::*;
use mocap_proto::mocap::*;

// TODO: Make sure the command always terminates.
pub fn run_command(
    req: &SupervisorRunRequest
) -> Result<SupervisorRunResponse> {
    let output = Command::new("/bin/bash")
        .arg("-c")
        .arg(req.command())
        .output()?;

    let mut res = SupervisorRunResponse::default();
    res.set_stdout(output.stdout);
    res.set_stderr(output.stderr);
    res.set_success(output.status.success());
    Ok(res)
}
