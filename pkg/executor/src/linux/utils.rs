use alloc::boxed::Box;
use std::future::Future;

use common::errors::*;

use crate::channel::oneshot;
use crate::linux::executor::Executor;
use crate::linux::join_handle::JoinHandle;

use super::thread_local::CurrentExecutorContext;

pub fn run<F: Future>(future: F) -> Result<F::Output> {
    let exec = Executor::create()?;
    exec.run(future)
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
