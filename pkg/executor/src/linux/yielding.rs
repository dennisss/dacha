use common::errors::*;
use sys::IoUringOp;

use crate::linux::io_uring::ExecutorOperation;

pub async fn yield_now() -> Result<()> {
    let op = ExecutorOperation::submit(IoUringOp::Noop).await?;
    let _ = op.wait().await?;
    Ok(())
}

pub(crate) async fn wake_polling_loop() {
    ExecutorOperation::submit(IoUringOp::Noop).await.unwrap();
}
