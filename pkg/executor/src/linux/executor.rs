use alloc::boxed::Box;
use alloc::vec::Vec;
use core::future::Future;
use core::pin::Pin;
use core::sync::atomic::{AtomicBool, Ordering};
use core::task::{Context, Poll, RawWaker, Waker};
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::{spawn, JoinHandle};
use std::time::Duration;

use base_error::*;
use common::hash::FastHasherBuilder;
use sys::{IoCompletionUring, IoSubmissionUring, IoUring, IoUringOp, IoUringResult};

use crate::linux::epoll::*;
use crate::linux::io_uring::*;
use crate::linux::options::{ExecutorOptions, ExecutorRunMode};
use crate::linux::task::{Task, TaskEntry, TaskState};
use crate::linux::timeout::ExecutorTimeouts;
use crate::linux::waker::create_waker;
use crate::stack_pinned::stack_pinned;

use super::thread_local::{CurrentExecutorContext, CurrentTaskContext};

pub type TaskId = u64;

// TODO: Move to sys:: ?
pub(super) type FileDescriptor = sys::c_int;

pub struct Executor {
    options: ExecutorOptions,

    shared: Arc<ExecutorShared>,

    io_uring_thread: JoinHandle<()>,
    epoll_thread: JoinHandle<()>,

    thread_pool: Vec<JoinHandle<()>>,
}

/// Shared data associated with the executor.
/// Instances are canonicaly Arc<Self> values.
pub(super) struct ExecutorShared {
    /// Whether or not the executor is allowed any new root tasks to be added
    /// (tasks not created by existing tasks). When false, all work will be
    /// done when tasks.is_empty().
    accepting_root_tasks: AtomicBool,

    /// Set of all actively running tasks.
    tasks: Mutex<HashMap<TaskId, Arc<TaskEntry>, FastHasherBuilder>>,

    next_task_id: Mutex<TaskId>,

    /// State used to implement async operations on top of the linux io_uring
    /// framework.
    pub(super) io_uring: ExecutorIoUring,

    pub(super) epoll: ExecutorEpoll,

    pub(super) timeouts: ExecutorTimeouts,

    // pub(super)
    /// List of tasks which need to be polled next.
    pending_queue: Mutex<VecDeque<TaskId>>,

    /// Notifier used to wait for changes to pending_queue.
    pending_queue_condvar: Condvar,
}

impl Executor {
    pub fn create(options: ExecutorOptions) -> Result<Self> {
        let shared = Arc::new(ExecutorShared {
            accepting_root_tasks: AtomicBool::new(true),

            tasks: Mutex::new(HashMap::with_hasher(FastHasherBuilder::default())),
            next_task_id: Mutex::new(1),

            io_uring: ExecutorIoUring::create()?,
            epoll: ExecutorEpoll::create()?,
            timeouts: ExecutorTimeouts::new(),

            pending_queue: Mutex::new(VecDeque::new()),
            pending_queue_condvar: Condvar::new(),
        });

        let mut thread_pool = vec![];

        // NOTE: The poller must be on a separate thread incase the main thread is
        // running a future that needs to park itself for I/O operation cancellation.
        let io_uring_thread = {
            let shared = shared.clone();
            Self::spawn_thread("exec::io_uring", move || Self::io_uring_thread_fn(shared))?
        };

        let epoll_thread = {
            let shared = shared.clone();
            Self::spawn_thread("exec::epoll", move || Self::epoll_thread_fn(shared))?
        };

        let num_threads = match options.thread_pool_size.clone() {
            Some(v) => v,
            None => sys::num_cpus()?,
        };

        for i in 0..num_threads {
            let shared = shared.clone();
            thread_pool.push(Self::spawn_thread(
                &format!("exec::pool_{}", i),
                move || Self::thread_fn(shared),
            )?);
        }

        Ok(Self {
            options,
            shared,
            thread_pool,
            io_uring_thread,
            epoll_thread,
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

        // No more root tasks will be added as we own the executor instance

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

            // The future should be dropped before we exit the task/executor contexts.
            drop(future);

            // NOTE: Removing the task will ensure that the task list will eventually become
            // empty so that worker threads can exit.
            shared.tasks.lock().unwrap().remove(&task_entry.id);
        }

        drop(task_context);
        drop(executor_context);

        Self::stop_accepting_root_tasks(&shared);

        if self.options.run_mode == ExecutorRunMode::StopAllOperations {
            shared.io_uring.shutdown();
            shared.epoll.shutdown();

            // All I/O operations should return a cancelled error on the next Poll because
            // we shut down the io_uring.
            let tasks = shared.tasks.lock().unwrap();
            for task_entry in tasks.values() {
                Self::wake_task_entry(&task_entry, false);
            }
        }

        if self.options.run_mode == ExecutorRunMode::StopAllOperations
            || self.options.run_mode == ExecutorRunMode::WaitForAllTasks
        {
            for thread in self.thread_pool {
                thread.join().unwrap();
            }

            shared.io_uring.shutdown();

            // NOTE: Even though all workers have been stopped, we may still have some
            // pending cancellation operations.
            self.io_uring_thread.join().unwrap();

            self.epoll_thread.join().unwrap();
        }

        Ok(output)
    }

    fn stop_accepting_root_tasks(shared: &ExecutorShared) {
        // TODO: Put inside of the mutex used for the pending queue.
        shared.accepting_root_tasks.store(false, Ordering::SeqCst);

        // For all worker threads to notice that accepting_root_tasks == false.
        shared.pending_queue_condvar.notify_all();
    }

    fn spawn_thread<F: FnOnce() + Sync + Send + 'static>(
        name: &str,
        f: F,
    ) -> Result<JoinHandle<()>> {
        // Wrap all executor threads in an abort.
        // This is so that if a single task panics, we notice this in the form of the
        // whole process ending. Otherwise we may end up in a situation where we are
        // blocked waiting for threads to finish.
        std::thread::Builder::new()
            .name(name.into())
            .spawn(|| {
                let mut aborter = AbortOnDrop::new();
                f();
                aborter.stop_abort();
            })
            .map_err(|e| Error::from(e))
    }

    /// Runs until all tasks spawned in the executor have finished running.
    /// This is a blocking call and also runs the main polling logic.
    fn io_uring_thread_fn(shared: Arc<ExecutorShared>) {
        Self::io_uring_thread_inner(shared).unwrap();
    }

    fn io_uring_thread_inner(shared: Arc<ExecutorShared>) -> Result<()> {
        let mut tasks_to_wake = HashSet::with_hasher(FastHasherBuilder::default());

        while !shared.io_uring.finished() {
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

    fn epoll_thread_fn(shared: Arc<ExecutorShared>) {
        Self::epoll_thread_inner(shared).unwrap();
    }

    fn epoll_thread_inner(shared: Arc<ExecutorShared>) -> Result<()> {
        let mut tasks_to_wake = HashSet::with_hasher(FastHasherBuilder::default());

        while !shared.epoll.finished() {
            tasks_to_wake.clear();
            shared.epoll.poll_events(&mut tasks_to_wake)?;

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
                if !shared.accepting_root_tasks.load(Ordering::SeqCst)
                    && shared.tasks.lock().unwrap().is_empty()
                {
                    // Stop running once we believe no other tasks will need to be executed in the
                    // future.
                    task_id = None;
                    break;
                }

                let mut pending_queue = shared.pending_queue.lock().unwrap();
                if let Some(next_task_id) = pending_queue.pop_front() {
                    task_id = Some(next_task_id);
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

            loop {
                if cancelled {
                    // Ensure that all operations are cleaned up before we remove the task entry
                    // so that any operation completions don't complain about non-existent
                    // tasks.
                    drop(future);

                    shared.tasks.lock().unwrap().remove(&task_id);
                    break;
                }

                let p = Future::poll(future.as_mut(), &mut context);

                match p {
                    Poll::Ready(()) => {
                        cancelled = true;
                        continue;
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

struct AbortOnDrop {
    should_abort: bool,
}

impl Drop for AbortOnDrop {
    fn drop(&mut self) {
        if self.should_abort {
            std::process::abort();
        }
    }
}

impl AbortOnDrop {
    pub fn new() -> Self {
        Self { should_abort: true }
    }

    pub fn stop_abort(&mut self) {
        self.should_abort = false;
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn task_is_eventually_removed() {

        //
    }
}
