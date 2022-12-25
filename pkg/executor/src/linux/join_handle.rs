use core::future::Future;
use std::sync::{Arc, Mutex};

use crate::{oneshot, Task};

// TODO: If any future is polled, we may need to change any old task id
// associated with it.
// - Or is it even possible to move
//
// Basically this is a spsc channel.

pub struct JoinHandle<T> {
    pub(super) task: Task,
    pub(super) receiver: oneshot::Receiver<T>,
}

impl<T> JoinHandle<T> {
    /// Attempts to cancel the task.
    ///
    /// The task is cancelled as soon as this function is called and the caller
    /// may wait for it to stop running by blocking on the returned future.
    pub fn cancel(mut self) -> impl Future<Output = Option<T>> {
        self.task.cancel();

        async move {
            let r = self.receiver.recv().await;
            match r {
                Ok(v) => Some(v),
                Err(_) => None,
            }
        }
    }

    /// TODO: If we disallow direct cancellation of a task, then we can gurantee
    /// that it always finishes without cancellation here.
    pub async fn join(mut self) -> T {
        // Because a task can only be cancelled via the cancel() method on this
        // instance, it must run to completion if we are calling this.
        self.receiver.recv().await.unwrap()
    }
}
