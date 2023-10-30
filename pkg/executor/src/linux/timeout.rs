use core::{future::Future, time::Duration};

use alloc::boxed::Box;
use common::errors::*;

use crate::future::{map, race};
use crate::linux::io_uring::ExecutorOperation;

const MAX_SLEEP_PRECISION: Duration = Duration::from_millis(1);

pub struct ExecutorTimeoutState {
    //
}

pub async fn sleep(duration: Duration) -> Result<()> {
    // TODO: Submitting an operation for every single operation can be rather
    // expensive (though it is likely we will have lots of timeouts).

    // Also want to prioritize having 2 slots in the ring for timeout ops.

    let op = ExecutorOperation::submit(sys::IoUringOp::Timeout { duration }).await?;
    let res = op.wait().await?;
    res.timeout_result()?;
    Ok(())
}

pub fn timeout<F: Future>(duration: Duration, f: F) -> impl Future<Output = Result<F::Output>> {
    race(
        map(Box::pin(f), |v| Ok(v)),
        map(Box::pin(sleep(duration)), |_| {
            Err(err_msg("Future timed out"))
        }),
    )
}
