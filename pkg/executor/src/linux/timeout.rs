use core::time::Duration;

use common::errors::*;

use crate::linux::io_uring::ExecutorOperation;

pub async fn sleep(duration: Duration) -> Result<()> {
    let op = ExecutorOperation::submit(sys::IoUringOp::Timeout { duration }).await?;
    let res = op.wait().await?;
    res.timeout_result()?;
    Ok(())
}
