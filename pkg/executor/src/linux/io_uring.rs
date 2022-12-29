use core::future::Future;
use core::marker::PhantomData;
use core::pin::Pin;
use core::task::{Context, Poll};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::thread::Thread;

use common::errors::*;
use sys::{
    IoCompletionUring, IoSubmissionUring, IoUring, IoUringCompletion, IoUringOp, IoUringResult,
};

use crate::linux::executor::{ExecutorShared, TaskId};
use crate::linux::waker::retrieve_task_entry;

use super::task::TaskEntry;
use super::thread_local::CurrentTaskContext;

/// Reserve 10% of the completion queue for storing cancellations of other
/// operations.
const CANCELATION_BUFFER_FRACTION: f32 = 0.1;

const WAKE_USER_DATA: u64 = u64::MAX;

pub(super) struct ExecutorIoUring {
    submissions: Mutex<ExecutorIoUringSubmissions>,
    completion_ring: Mutex<IoCompletionUring>,
}

struct ExecutorIoUringSubmissions {
    /// Maximum number of operations we will allow to be pending at a time.
    max_pending_operations: usize,

    /// Maximum number of non-cancel operations we are allowed to have pending
    /// at a time.
    max_non_cancellation_operations: usize,

    submission_ring: IoSubmissionUring,

    /// Set of currently active operations in the io_uring.
    ///
    /// TODO: Use a slab. Tasks can have locks on slab entries because there
    /// will only ever be on thing accessing it.
    operations: HashMap<u64, ExecutorOperationState>,

    next_operation_id: u64,

    /// Tasks which are blocked because we currently have too many operations in
    /// flight.
    ///
    /// Once operations have finished, we will try waking up some of
    /// these.
    ///
    /// TODO: we need to ensure that this is always properly cleaned up.
    ///
    /// TODO: This needs to be a priority queue?
    blocked_tasks: HashSet<TaskId>,
}

/// Entry containing the current status of an ongoing operation.
/// Each of these instances corresponds to a single ExecutorOperation instance
/// that's still alive in a task.
struct ExecutorOperationState {
    task_id: TaskId,

    /// If true, the task that created this operation no longer needs it and it
    /// can be cleaned up when it completes.
    detached: bool,

    /// If this operation was recently completed, this is the result of that.
    result: Option<IoUringResult>,
}

impl ExecutorIoUring {
    pub fn create() -> Result<Self> {
        let (submission_ring, completion_ring) = IoUring::create()?.split();

        let max_pending_operations = completion_ring.capacity();

        let submissions = Mutex::new(ExecutorIoUringSubmissions {
            max_pending_operations,
            max_non_cancellation_operations: ((max_pending_operations as f32)
                * (1. - CANCELATION_BUFFER_FRACTION))
                as usize,
            submission_ring,
            operations: HashMap::new(),
            next_operation_id: 1,
            blocked_tasks: HashSet::new(),
        });

        Ok(Self {
            submissions,
            completion_ring: Mutex::new(completion_ring),
        })
    }

    /// Waits until at least one operation is complete and retrieves the set of
    /// tasks that need to be woken up.
    ///
    /// NOTE: We strictly append to 'tasks_to_wake'.
    pub fn poll_events(&self, tasks_to_wake: &mut HashSet<TaskId>) -> Result<()> {
        let mut completion_ring = self.completion_ring.lock().unwrap();
        completion_ring.wait(Some(std::time::Duration::from_secs(1)))?;

        let mut submissions = self.submissions.lock().unwrap();

        while let Some(completion) = completion_ring.retrieve() {
            if completion.user_data == WAKE_USER_DATA {
                continue;
            }

            let mut op = submissions
                .operations
                .get_mut(&completion.user_data)
                .ok_or_else(|| err_msg("Unknown operation completed"))?;

            if op.result.is_some() {
                return Err(err_msg("Operation completed multiple times"));
            }

            op.result = Some(completion.result);

            if op.detached {
                submissions.operations.remove(&completion.user_data);
                continue;
            }

            tasks_to_wake.insert(op.task_id);
        }

        // TODO: If we have space, also allow blocked tasks to proceed.

        Ok(())
    }

    /// Triggers any callers to poll_events() to unblock shortly after this is
    /// called.
    pub fn wake_poller(&self) -> Result<()> {
        let mut submissions = self.submissions.lock().unwrap();

        // TODO: Verify that we always have space in the completion queue to get
        // this entry.

        unsafe {
            submissions
                .submission_ring
                .submit(IoUringOp::Noop, WAKE_USER_DATA)?;
        }

        Ok(())
    }
}

pub struct ExecutorOperation<'a, 'b> {
    /// TODO: We can remove this if tasks are always dropped within an executor
    /// context.
    executor_shared: Arc<ExecutorShared>,

    id: u64,

    /// If true, then this operation completed normally and is no longer being
    /// polled.
    done: bool,

    /// Describes how we should cancel this operation if we no longer need it.
    cancellation_mode: ExecutorOperationCancelMode,

    lifetime: PhantomData<&'a ()>,
    lifetime2: PhantomData<&'b ()>,
}

#[derive(Clone, Copy)]
enum ExecutorOperationCancelMode {
    Nothing,

    /// TODO: Correct this (it is no longer used as we detach cancellations
    /// right away)
    ///
    /// In this mode, we will simply detach the operation so that the executor
    /// cleans it up once it is reported to be complete.
    ///
    /// (used for 'Cancel' operations so that we avoid cancelling cancellations
    /// which we assume as fast enough that it doesnt' matter).
    DetachOnly,

    /// Cancel the operation and detach it from the task. The executor will
    /// clean it up later in the future.
    ///
    /// (used for normal operations which don't reference any task data).
    CancelAndDetach,

    /// Cancel the operation and block the current task/thread until it has been
    /// completed.
    ///
    /// The operation MUST be marked as completed (or failed) by the kernel
    /// before we are allowed to proceed with running the task
    ///
    /// (used for any operation that references data owned by the task).
    CancelAndWait,
}

/*
TODO: We need to check if the task id in the context of an operation hasn't changed (we would need to change who we are waiting for).
*/

impl<'a, 'b> Drop for ExecutorOperation<'a, 'b> {
    fn drop(&mut self) {
        // TODO: May already be detached in the case of a cancellation.
        if self.done {
            return;
        }

        // Must submit a cancelation of the operation and then park the thread
        // until it is cancelled.
        match self.cancellation_mode {
            ExecutorOperationCancelMode::Nothing => Ok(()),
            ExecutorOperationCancelMode::DetachOnly => self.detach(false, true),
            ExecutorOperationCancelMode::CancelAndDetach => self.detach(true, false),
            ExecutorOperationCancelMode::CancelAndWait => self.detach(true, true),
        }
        .unwrap();

        // TODO: We must mininally mark it as dropped so that the executor can
        // clean it up (or if it is already completed, we need to )
    }
}

impl<'a, 'b> ExecutorOperation<'a, 'b> {
    /// NOTE: The operation must outlive all data referenced in the operation.
    pub fn submit(
        op: IoUringOp<'a, 'b>,
    ) -> impl Future<Output = Result<ExecutorOperation<'a, 'b>>> {
        ExecutorOperationSubmitFuture {
            op: Some(op),
            initially_detached: false,
        }
    }

    pub fn wait(self) -> impl Future<Output = Result<IoUringResult>> + 'a
    where
        'b: 'a,
    {
        ExecutorOperationWaitFuture { op: self }
    }

    /// NOTE: This assumes !self.done
    fn detach(&self, mut cancel: bool, wait: bool) -> Result<()> {
        let current_task = CurrentTaskContext::current().unwrap();

        loop {
            {
                let mut submissions = self.executor_shared.io_uring.submissions.lock().unwrap();
                let mut entry = submissions.operations.get_mut(&self.id).unwrap();
                if entry.result.is_some() {
                    // Already done so we can just remove it.
                    submissions.operations.remove(&self.id);
                    return Ok(());
                }

                if !cancel && !wait {
                    entry.detached = true;
                    break;
                }
            }

            if cancel {
                cancel = false; // Only cancel once

                ExecutorOperationSubmitFuture {
                    op: Some(IoUringOp::Cancel { user_data: self.id }),
                    initially_detached: true,
                }
                .poll_with_task(&current_task)?;
            }

            if wait {
                current_task.park_on_current_thread();
            }
        }

        Ok(())
    }
}

struct ExecutorOperationSubmitFuture<'a, 'b> {
    op: Option<IoUringOp<'a, 'b>>,
    initially_detached: bool,
}

impl<'a, 'b> ExecutorOperationSubmitFuture<'a, 'b> {
    fn poll_with_result(&mut self, context: &mut Context<'_>) -> Result<ExecutorOperation<'a, 'b>> {
        let task_entry = retrieve_task_entry(context)
            .ok_or_else(|| err_msg("Not running inside an executor"))?;
        self.poll_with_task(task_entry)
    }

    fn poll_with_task(&mut self, task_entry: &TaskEntry) -> Result<ExecutorOperation<'a, 'b>> {
        let shared = task_entry.executor_shared.clone();
        let task_id = task_entry.id;

        let mut submissions = shared.io_uring.submissions.lock().unwrap();

        let op_id = submissions.next_operation_id;
        submissions.next_operation_id += 1;

        // The future will only ever get polled once.
        let op = self.op.take().unwrap();

        let must_wait = op.try_into_static().is_none();

        let is_cancellation = if let IoUringOp::Cancel { .. } = op {
            true
        } else {
            false
        };

        // NOTE: This also implicitly prohibits users from submitting cancelations as
        // they can't create detached ops.
        if is_cancellation && !self.initially_detached {
            return Err(err_msg("Non-detached cancellations not allowed"));
        }

        unsafe {
            submissions.submission_ring.submit(op, op_id)?;
        }

        submissions.operations.insert(
            op_id,
            ExecutorOperationState {
                task_id,
                detached: self.initially_detached,
                result: None,
            },
        );

        Ok(ExecutorOperation {
            executor_shared: shared.clone(),
            id: op_id,
            done: false,
            cancellation_mode: {
                if must_wait {
                    assert!(!self.initially_detached);
                    ExecutorOperationCancelMode::CancelAndWait
                } else if is_cancellation {
                    ExecutorOperationCancelMode::Nothing
                } else {
                    assert!(!self.initially_detached);
                    ExecutorOperationCancelMode::CancelAndDetach
                }
            },
            lifetime: PhantomData,
            lifetime2: PhantomData,
        })
    }
}

impl<'a, 'b> Future for ExecutorOperationSubmitFuture<'a, 'b> {
    type Output = Result<ExecutorOperation<'a, 'b>>;

    fn poll(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };

        Poll::Ready(this.poll_with_result(context))
    }
}

struct ExecutorOperationWaitFuture<'a, 'b> {
    op: ExecutorOperation<'a, 'b>,
}

impl<'a, 'b> Future for ExecutorOperationWaitFuture<'a, 'b> {
    type Output = Result<IoUringResult>;

    fn poll(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };
        let mut submissions = this.op.executor_shared.io_uring.submissions.lock().unwrap();

        let mut op = match submissions.operations.get_mut(&this.op.id) {
            Some(op) => op,
            None => {
                return Poll::Ready(Err(err_msg("Operation disappeared")));
            }
        };

        match op.result.take() {
            Some(res) => {
                // TODO: Upon removal, allow any other blocked tasks to issue submissions.
                submissions.operations.remove(&this.op.id);
                this.op.done = true;

                Poll::Ready(Ok(res))
            }
            None => Poll::Pending,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cancellation_test() {

        // crate::run(async )
    }
}

/*
A few different things to test:
- Cancel a simple future like a timeout
- Cancel a complex one like a read from a pipe

- If we test inside of this module, we can assert that operations have disappeared from our map.

- Using a join handle to retrieve the result of a task
- Cancelling said task

- Would be nice to simulate some intersting scenarios
    - Like timeout an Accept() on a TcpListener

*/
