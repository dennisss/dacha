#[cfg(feature = "alloc")]
use alloc::boxed::Box;
#[cfg(feature = "alloc")]
use alloc::string::String;
#[cfg(feature = "alloc")]
use alloc::vec::Vec;
use std::cell::Cell;
use std::future::Future;
use std::marker::PhantomData;
use std::pin::Pin;
use std::string::ToString;
use std::sync::{Arc, RwLock};
use std::task::{Context, Poll};

use base_error::*;
use common::failure::ResultExt;

use crate::channel;
use crate::child_task::ChildTask;
use crate::{spawn, JoinHandle};

pub struct TaskBundle<'a> {
    active: Arc<RwLock<bool>>,
    handles: Vec<JoinHandle<()>>,
    scope: PhantomData<&'a ()>,
}

impl<'a> TaskBundle<'a> {
    pub fn new() -> Self {
        Self {
            active: Arc::new(RwLock::new(true)),
            handles: vec![],
            scope: PhantomData,
        }
    }

    pub fn add<'b, F: Future<Output = ()> + Send + 'b>(&mut self, f: F)
    where
        'a: 'b,
    {
        let fboxed: Pin<Box<dyn Future<Output = ()> + Send>> = Box::pin(f);
        let fstatic: Pin<Box<dyn Future<Output = ()> + Send + 'static>> =
            unsafe { core::mem::transmute(fboxed) };
        self.handles.push(spawn(TaskFuture {
            active: self.active.clone(),
            fut: fstatic,
        }));
    }

    pub async fn join(mut self) {
        for handle in self.handles.drain(..) {
            handle.join().await;
        }
    }

    // TODO: Enable cancelling cancelable futures.
}

impl<'a> Drop for TaskBundle<'a> {
    // If the bundle is dropped before all tasks after completed, it will block
    // until they are all done.
    fn drop(&mut self) {
        *self.active.write().unwrap() = false;
    }
}

// TODO: Also do this for the ChildTask class?
struct TaskFuture {
    active: Arc<RwLock<bool>>,
    fut: Pin<Box<dyn Future<Output = ()> + Send + 'static>>,
}

impl Future for TaskFuture {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        // Must hold a reader lock shared with the main bundle to run.
        let active = self.active.clone();
        let active_guard = active.read().unwrap();
        if !*active_guard {
            return Poll::Ready(());
        }

        self.fut.as_mut().poll(cx)
    }
}

/// A collection of concurrently running tasks which each returns a Ok/Err
/// return value.
pub struct TaskResultBundle {
    tasks: Vec<(String, ChildTask)>,
    num_done: usize,
    sender: channel::Sender<(usize, Result<()>)>,
    receiver: channel::Receiver<(usize, Result<()>)>,
}

impl TaskResultBundle {
    /// Creates a new empty bundle of tasks.
    pub fn new() -> Self {
        let (sender, receiver) = channel::unbounded();

        Self {
            tasks: vec![],
            num_done: 0,
            sender,
            receiver,
        }
    }

    /// Adds a task to the bundle and immediately starts running it.
    pub fn add<F: Future<Output = Result<()>> + Send + 'static>(
        &mut self,
        task_name: &str,
        future: F,
    ) -> &mut Self {
        let task_i = self.tasks.len();
        let sender = self.sender.clone();

        let child = ChildTask::spawn(async move {
            let _ = sender.send((task_i, future.await)).await;
        });

        self.tasks.push((task_name.to_string(), child));
        self
    }

    /// Waits for either all tasks to finish successfully or for one of the
    /// tasks to fail. In the case of a failure, only the error from the
    /// first task which failed will be returned. If more than one task failed,
    /// later failures will be silently ignored.
    pub async fn join(&mut self) -> Result<()> {
        if self.num_done == self.tasks.len() {
            return Err(err_msg("All tasks are already complete"));
        }

        loop {
            let (task_i, result) = self.receiver.recv().await?;
            self.num_done += 1;
            result.with_context(|e| format!("Task {} failed: {}", self.tasks[task_i].0, e))?;

            if self.num_done == self.tasks.len() {
                break;
            }
        }

        Ok(())
    }
}
