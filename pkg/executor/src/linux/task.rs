use alloc::boxed::Box;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::thread::Thread;

use base_error::*;

use crate::linux::executor::{Executor, ExecutorShared, TaskId};
use crate::linux::thread_local::CurrentTaskContext;

/// Reference to a task spawned in an executor.
/// This can be used similarly to a waker.
#[derive(Clone)]
pub struct Task {
    pub(super) entry: Arc<TaskEntry>,
}

impl Task {
    pub fn current() -> Result<Self> {
        let entry =
            CurrentTaskContext::current().ok_or_else(|| err_msg("Not running in a task"))?;
        Ok(Self { entry })
    }

    pub fn id(&self) -> TaskId {
        self.entry.id
    }

    /// If the task isn't already complete, this triggers it to be repolled at
    /// least one more time.
    pub fn wake(&self) {
        Executor::wake_task_entry(&self.entry, false);
    }

    // TODO: Disallow self cancelation (mainly because a JoinHandle would never
    // POll::ready() in this case).
    pub(super) fn cancel(&self) {
        Executor::wake_task_entry(&self.entry, true);
    }
}

/// Data stored in the executor to track the progress and state of a task.
///
/// When a task is being polled, the waker contains a pointer to the TaskEntry.
pub(super) struct TaskEntry {
    pub id: TaskId,

    pub state: Mutex<TaskState>,

    /// Back-reference to the executor in which this task is located.
    pub executor_shared: Arc<ExecutorShared>,
}

impl TaskEntry {
    /// Parks the calling thread until this task is woken up again.
    pub fn park_on_current_thread(&self) {
        loop {
            {
                let mut state = self.state.lock().unwrap();
                if state.dirty {
                    state.dirty = false;
                    return;
                }

                state.parked_thread = Some(std::thread::current());
            }

            std::thread::park();
        }
    }
}

pub(super) struct TaskState {
    /// Whether or not this task is scheduled to run on some thread (or is
    /// currently running on some thread).
    ///
    /// This is set when the task is enqueued to run on some thread and is unset
    /// after a thread is done polling it.
    ///
    /// This is used to ensure that tasks never attempt to be assigned to
    /// multiple threads at the same time.
    ///
    /// NOTE: Cancelled tasks continuously stay scheduled until their TaskEntry
    /// is completely deallocated.
    pub scheduled: bool,

    /// Reference to a dedicated thread running just this task.
    ///
    /// When set, this thread is assumed to be currently parked and needs to be
    /// unparked whenever the task needs to be woken up to continue running
    /// the task.
    ///
    /// This field effectively allows a thread to continously keep 'scheduled'
    /// set to true and use blocking to wait for a future. This mainly has the
    /// following usecases:
    /// - Allows the root future to execute in the main application thread.
    /// - Allows waiting on events in the Drop method of operations in the rare
    ///   case that some cleanup is required to proceed with a task.
    pub parked_thread: Option<Thread>,

    /// Main future for this task. This is taken by the thread running the
    /// task.
    pub future: Option<Pin<Box<dyn Future<Output = ()> + Send + 'static>>>,

    /// If true, then since this thread started executing, new events were
    /// received that would have woken it up again.
    pub dirty: bool,

    /// If true, this task is cancelled and should no longer be polled.
    ///
    /// NOTE: A task is only allowed to be cancelled through its JoinHandle
    /// (this is mainly a necessary requirement to guarantee that .join() always
    /// returns a result unless the join handle is dropped).
    pub cancelled: bool,
}
