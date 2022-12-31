use alloc::boxed::Box;
use alloc::vec::Vec;
use core::future::Future;
use core::pin::Pin;
use core::sync::atomic::{AtomicBool, Ordering};
use core::task::{Context, Poll, RawWaker, Waker};
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::{spawn, JoinHandle};

use common::errors::*;
use sys::{
    Epoll, EpollEvent, EpollEvents, EpollOp, IoCompletionUring, IoSubmissionUring, IoUring,
    IoUringOp, IoUringResult,
};

use crate::linux::io_uring::*;
use crate::linux::task::{Task, TaskEntry, TaskState};
use crate::linux::waker::create_waker;
use crate::stack_pinned::stack_pinned;

use super::thread_local::{CurrentExecutorContext, CurrentTaskContext};

pub type TaskId = u64;

// TODO: Move to sys:: ?
pub(super) type FileDescriptor = sys::c_int;

pub struct Executor {
    shared: Arc<ExecutorShared>,

    thread_pool: Vec<JoinHandle<()>>,
}

/*
MVP: Call io_uring_enter() after every single entry is added.
- Then we don't need to worry about the # of submissions but only the number of in-flight ones.
- Later:
    -
- If we have hit the max in-flight operations, how do we wait for no longer being in this state:
    - We need to have a set of tasks which want to be woken up for this.
    - they can retry.
- If an in-progress operation is active, we must wait for a cancellation before a task can complete.
    - How to cancel something:
        - Submit a cancelation (this is also an operation, so we limit to 95% of the overall queue size)


Some challenges:
- If we are using io_uring_enter to block on submissions of entries,

*/

/// Shared data associated with the executor.
/// Instances are canonicaly Arc<Self> values.
pub(super) struct ExecutorShared {
    /// Whether or not the executor is running.
    /// Initially this is true and is set to false when the main task completes.
    running: AtomicBool,

    /// Set of all actively running tasks.
    tasks: Mutex<HashMap<TaskId, Arc<TaskEntry>>>,

    next_task_id: Mutex<TaskId>,

    /// State used to implement async operations on top of the linux io_uring
    /// framework.
    pub(super) io_uring: ExecutorIoUring,

    /// List of tasks which need to be polled next.
    pending_queue: Mutex<VecDeque<TaskId>>,

    /// Notifier used to wait for changes to pending_queue.
    pending_queue_condvar: Condvar,
}

impl Executor {
    pub fn create() -> Result<Self> {
        let shared = Arc::new(ExecutorShared {
            running: AtomicBool::new(true),

            tasks: Mutex::new(HashMap::new()),
            next_task_id: Mutex::new(1),

            io_uring: ExecutorIoUring::create()?,

            pending_queue: Mutex::new(VecDeque::new()),
            pending_queue_condvar: Condvar::new(),
        });

        let mut thread_pool = vec![];

        // NOTE: The poller must be on a separate thread incase the main thread is
        // running a future that needs to park itself for cancellation.
        //
        // TODO: Do something with the Result returned by this.
        {
            let shared = shared.clone();
            thread_pool.push(spawn(move || Self::polling_thread_fn(shared).unwrap()));
        }

        // TODO: Base on the number of CPU cores.
        for i in 0..4 {
            let shared = shared.clone();
            thread_pool.push(spawn(move || Self::thread_fn(shared)));
        }

        Ok(Self {
            shared,
            thread_pool,
        })
    }

    /// Runs the given future on the executor and blocks until it is complete.
    ///
    /// - The given future is
    pub fn run<F: Future>(self, future: F) -> Result<F::Output> {
        let mut shared = self.shared.clone();

        // Create a new task.
        // TODO: Dedup this logic.
        let task_entry = {
            let task_id = {
                let mut next_id = shared.next_task_id.lock().unwrap();
                let id = *next_id;
                *next_id += 1;
                id
            };

            let entry = Arc::new(TaskEntry {
                id: task_id,
                state: Mutex::new(TaskState {
                    scheduled: true, // Always running on main thread.
                    future: None,
                    dirty: false,
                    cancelled: false,
                    parked_thread: None,
                }),
                executor_shared: shared.clone(),
            });

            shared.tasks.lock().unwrap().insert(task_id, entry.clone());
            entry
        };

        // Poll the main future.
        let output;

        let executor_context = CurrentExecutorContext::new(&shared);
        let task_context = CurrentTaskContext::new(&task_entry);
        {
            common::futures::pin_mut!(future);

            let waker = create_waker(task_entry.clone());
            let mut context = Context::from_waker(&waker);

            loop {
                match future.as_mut().poll(&mut context) {
                    Poll::Ready(v) => {
                        output = v;
                        break;
                    }
                    Poll::Pending => {
                        task_entry.park_on_current_thread();
                    }
                }
            }

            // The future should be dropped before we exit the executor context.
            drop(future);
        }

        drop(task_context);
        drop(executor_context);

        // TODO: Need a grace period:
        // - First wait some time for all the tasks to finish
        // - Then actively stop polling futures.
        // - Finally

        Self::shutdown(&shared);

        // TODO: If we stop the io_uring thread, then it is possible that any futures
        // that still need to be dropped and cancelled will never finish.
        for thread in self.thread_pool {
            // TODO: We may want to cancel threads if they are stuck on some long running
            // blocking computation.
            thread.join().unwrap();
        }

        Ok(output)
    }

    fn shutdown(shared: &ExecutorShared) {
        // TODO: Put inside of the mutex used for the pending queue.
        shared.running.store(false, Ordering::SeqCst);

        // For all worker threads to notice that running == false.
        shared.pending_queue_condvar.notify_all();

        // Force the event loop to wake up and stop as running == false.
        shared.io_uring.wake_poller().unwrap();
    }

    /// Runs until all tasks spawned in the executor have finished running.
    /// This is a blocking call and also runs the main polling logic.
    fn polling_thread_fn(shared: Arc<ExecutorShared>) -> Result<()> {
        let mut tasks_to_wake = HashSet::new();

        // TODO: Also stop if any of the threads paniced.
        while shared.running.load(Ordering::SeqCst) {
            tasks_to_wake.clear();
            shared.io_uring.poll_events(&mut tasks_to_wake)?;

            let tasks = shared.tasks.lock().unwrap();

            for task_id in tasks_to_wake.drain() {
                let task_entry = tasks
                    .get(&task_id)
                    .ok_or_else(|| err_msg("Task disappeared"))?;

                Self::wake_task_entry(task_entry.as_ref(), false);
            }
        }

        Ok(())
    }

    pub(super) fn wake_task_entry(task_entry: &TaskEntry, cancel: bool) {
        let mut task_state = task_entry.state.lock().unwrap();

        // NOTE: The actual cancellation (dropping of the future) always happens on a
        // worker thread as dropping a task may involve a lot of computation (e.g.
        // cleaning up I/O operations). This also ensures that operations like
        // additional task spawning is still executed in the context of an executor.
        if cancel {
            task_state.cancelled = true;
        }

        // Also schedule tasks which aren't already running.
        if task_state.scheduled {
            if task_state.dirty {
                return;
            }

            task_state.dirty = true;

            if let Some(thread) = task_state.parked_thread.take() {
                thread.unpark();
            }

            return;
        }

        // Don't schedule a task which has already finished. This case would only be hit
        // if a user is retaining a copy of an old 'Task' or 'JoinHandle' struct.
        if task_state.future.is_none() {
            return;
        }

        task_state.scheduled = true;

        let shared = &task_entry.executor_shared;
        shared
            .pending_queue
            .lock()
            .unwrap()
            .push_back(task_entry.id);
        shared.pending_queue_condvar.notify_one();
    }

    pub(super) fn spawn(
        shared: &Arc<ExecutorShared>,
        future: Pin<Box<dyn Future<Output = ()> + Send + 'static>>,
    ) -> Task {
        let task_id = {
            let mut next_id = shared.next_task_id.lock().unwrap();
            let id = *next_id;
            *next_id += 1;
            id
        };

        let entry = Arc::new(TaskEntry {
            id: task_id,
            state: Mutex::new(TaskState {
                scheduled: true, // We immediately push it onto the pending_queue.
                future: Some(future),
                dirty: false,
                cancelled: false,
                parked_thread: None,
            }),
            executor_shared: shared.clone(),
        });

        shared.tasks.lock().unwrap().insert(task_id, entry.clone());

        {
            let mut pending_queue = shared.pending_queue.lock().unwrap();
            pending_queue.push_back(task_id);
        }

        shared.pending_queue_condvar.notify_one();

        Task { entry }
    }

    /// Entry point for worker threads which poll futures.
    fn thread_fn(shared: Arc<ExecutorShared>) {
        let executor_context = CurrentExecutorContext::new(&shared);
        loop {
            let task_id;
            loop {
                let mut pending_queue = shared.pending_queue.lock().unwrap();
                if let Some(next_task_id) = pending_queue.pop_front() {
                    task_id = Some(next_task_id);
                    break;
                } else if !shared.running.load(Ordering::SeqCst) {
                    task_id = None;
                    break;
                } else {
                    pending_queue = shared.pending_queue_condvar.wait(pending_queue).unwrap();
                }
            }

            let task_id = match task_id {
                Some(id) => id,
                None => break,
            };

            // TODO: Must ensure no other thread is running this task.
            let task_entry = {
                let entries = shared.tasks.lock().unwrap();
                entries.get(&task_id).unwrap().clone()
            };

            // NOTE: This is declared before the 'future' so that any drops of the future
            // always occur with a task context.
            let task_context = CurrentTaskContext::new(&task_entry);

            let (mut future, mut cancelled) = {
                let mut state = task_entry.state.lock().unwrap();
                state.dirty = false;

                assert!(state.scheduled);

                // NOTE: This should never fail as only one thread polls the future at a time
                // and we put the future back before we allow the task to be scheduled on
                // another thread.
                (state.future.take().unwrap(), state.cancelled)
            };

            let waker = create_waker(task_entry.clone());
            let mut context = Context::from_waker(&waker);

            /*
            TODO: The smartest way to completely cancel a future would be to:
            - We should know which operations it is waiting for.
            - Cancel them first.
            - Then the drop() destructors should be
            */

            while !cancelled {
                let p = Future::poll(future.as_mut(), &mut context);

                match p {
                    Poll::Ready(()) => {
                        // Ensure that all operations are cleaned up before we remove the task entry
                        // so that any operation completions don't complain about non-existent
                        // tasks.
                        drop(future);

                        shared.tasks.lock().unwrap().remove(&task_id);
                        break;
                    }
                    Poll::Pending => {
                        let mut state = task_entry.state.lock().unwrap();

                        cancelled = state.cancelled;

                        // Re-poll the task on the same thread if it received more events.
                        // NOTE: Cancelled futures are also 'dirty'.
                        if state.dirty {
                            state.dirty = false;
                            continue;
                        }

                        // We don't need to poll the task in the near future so put it back into the
                        // pool of schedulable tasks.
                        state.scheduled = false;
                        state.future = Some(future);
                        break;
                    }
                }
            }
        }
    }
}
