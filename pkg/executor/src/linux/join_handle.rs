use core::future::Future;
use std::sync::{Arc, Mutex};

use crate::channel::oneshot;
use crate::{Task, TaskId};

// TODO: If any future is polled, we may need to change any old task id
// associated with it.
// - Or is it even possible to move
//
// Basically this is a spsc channel.

pub struct JoinHandle<T> {
    task: Task,
    receiver: Option<oneshot::Receiver<T>>,
    attached: bool,
}

impl<T> Drop for JoinHandle<T> {
    fn drop(&mut self) {
        if self.attached {
            self.task.cancel();
        }
    }
}

impl<T> JoinHandle<T> {
    pub(super) fn new(task: Task, receiver: oneshot::Receiver<T>) -> Self {
        Self {
            task,
            receiver: Some(receiver),
            attached: false,
        }
    }

    pub fn task(&self) -> &Task {
        &self.task
    }

    pub fn attach(&mut self) {
        self.attached = true;
    }

    /// Attempts to cancel the task.
    ///
    /// The task is cancelled as soon as this function is called and the caller
    /// may wait for it to stop running by blocking on the returned future.
    pub fn cancel(mut self) -> impl Future<Output = Option<T>> {
        self.task.cancel();
        self.attached = false;

        async move {
            let r = self.receiver.take().unwrap().recv().await;
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
        let result = self.receiver.take().unwrap().recv().await.unwrap();

        // Because we received a value, the task has already terminated.
        self.attached = false;

        result
    }
}
