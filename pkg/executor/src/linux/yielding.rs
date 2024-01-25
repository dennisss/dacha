use base_error::*;
use sys::IoUringOp;

use crate::linux::io_uring::ExecutorOperation;

pub async fn yield_now() -> Result<()> {
    let op = ExecutorOperation::submit(IoUringOp::Noop).await?;
    let _ = op.wait().await?;
    Ok(())
}
