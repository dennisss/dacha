use base_error::*;
use sys::IoUringOp;

use crate::linux::io_uring::ExecutorOperation;

pub async fn yield_now() -> Result<()> {
    // TODO: Make a cheaper yield that doesn't require submitting an op
    let op = ExecutorOperation::submit(IoUringOp::Noop).await?;
    let _ = op.wait().await?;
    Ok(())
}
