use std::process::Command;

use common::errors::*;
use mocap_proto::mocap::*;

// TODO: Make sure the command always terminates.
pub fn run_command(
    req: &CameraSupervisorRunRequest
) -> Result<CameraSupervisorRunResponse> {
    let output = Command::new("/bin/cat")
        .arg("bash")
        .arg("-c")
        .arg(req.command())
        .output()?;

      
    let mut res = CameraSupervisorRunResponse::default();
    res.set_stdout(output.stdout);
    res.set_stderr(output.stderr);
    res.set_success(output.status.success());
    Ok(res)
}
