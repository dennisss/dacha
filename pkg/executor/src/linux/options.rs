#[derive(Default)]
pub struct ExecutorOptions {
    pub thread_pool_size: Option<usize>,

    /// Defines the behavior of the executor after the root future passed to
    /// run() is complete.
    ///
    /// NOTE: This functionality is only guaranteed if the executor exits
    /// succeessfully.
    pub run_mode: ExecutorRunMode,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ExecutorRunMode {
    /// After the root future is complete, the executor will immediately return
    /// to the caller but any tasks still executing in the background will
    /// continue to run.
    ///
    /// Running in this mode is only suitable if you expect the program to exit
    /// immediately after the executor is done (as otherwise all the executor
    /// resources will continue consuming CPU/memory time potentially
    /// indefinitely).
    DetachTasks,

    /// All tasks that are currently scheduled on the executor must finish
    /// running before we return to the caller.
    WaitForAllTasks,

    /// Stops all tasks by cancelling all current/future I/O operations. The
    /// tasks themselves are NOT cancelled and will continue running until
    ///
    /// After the I/O operations are cancelled, this behaves similarly to
    /// WaitForAllTasks.
    StopAllTasks,
}

impl Default for ExecutorRunMode {
    fn default() -> Self {
        Self::DetachTasks
    }
}
