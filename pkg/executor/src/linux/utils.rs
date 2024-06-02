use alloc::boxed::Box;
use core::time::Duration;
use std::future::Future;

use base_error::*;

use crate::channel::oneshot;
use crate::linux::executor::Executor;
use crate::linux::join_handle::JoinHandle;
use crate::ExecutorOptions;

use super::thread_local::CurrentExecutorContext;

const GRACEFUL_SHUTDOWN_TIMEOUT: Duration = Duration::from_millis(1000);

pub fn run<F: Future>(future: F) -> Result<F::Output> {
    let exec = Executor::create(ExecutorOptions::default())?;
    exec.run(future)
}

pub fn run_main<F: Future>(future: F) -> Result<F::Output> {
    run(async move {
        let v = future.await;

        crate::signals::trigger_shutdown();

        // TODO: We no longer use this as a shutdown mechanism.
        crate::timeout(
            GRACEFUL_SHUTDOWN_TIMEOUT,
            crate::signals::wait_for_shutdowns(),
        )
        .await;

        v
    })
}

pub fn spawn<F: Future<Output = T> + Send + 'static, T: Send + 'static>(
    future: F,
) -> JoinHandle<T> {
    let executor_shared = CurrentExecutorContext::current().expect("Not running in an executor");

    let (sender, receiver) = oneshot::channel();

    let task = Executor::spawn(
        &executor_shared,
        Box::pin(async move {
            let value = future.await;
            let _ = sender.send(value);
        }),
    );

    JoinHandle::new(task, receiver)
}

/*
Key tests:
- Able to cancel a complex operation like an I/O
- Verify that by default a join handle is detached.
*/
