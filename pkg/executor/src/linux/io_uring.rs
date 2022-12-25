use core::future::Future;
use core::marker::PhantomData;
use core::pin::Pin;
use core::task::{Context, Poll};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use common::errors::*;
use sys::{
    IoCompletionUring, IoSubmissionUring, IoUring, IoUringCompletion, IoUringOp, IoUringResult,
};

use crate::linux::executor::{ExecutorShared, TaskId};
use crate::linux::waker::retrieve_task_entry;

/// Reserve 10% of the completion queue for storing cancellations of other
/// operations.
const CANCELATION_BUFFER_FRACTION: f32 = 0.1;

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

    // There are two types of tasks. 1 is a cancellation and 2 is a normal one.
    /*
    When a cancel suceeds, we need to tell the thread to unpark
    => So we mainly need to know the thread id.

    If an operation completes,
    - Need to avoid trying to send it to a parked task
    - So during cancelation, the task must be marked as parked.
    -

    One challenge:
    - Seeking on a very slow or broken disk can block threads for a while.

    Other things:
    - A task should have some amount of thread affinity.
    */
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
            let mut op = submissions
                .operations
                .get_mut(&completion.user_data)
                .ok_or_else(|| err_msg("Unknown operation completed"))?;

            if op.result.is_some() {
                return Err(err_msg("Operation completed multiple times"));
            }

            op.result = Some(completion.result);

            tasks_to_wake.insert(op.task_id);
        }

        // TODO: If we have space, also allow blocked tasks to proceed.

        Ok(())
    }
}

pub struct ExecutorOperation<'a> {
    /// TODO: We can remove this if tasks are always dropped within an executor
    /// context.
    executor_shared: Arc<ExecutorShared>,

    id: u64,

    /// If true, then this operation completed normally and is no longer being
    /// polled.
    done: bool,

    must_cancel: bool,

    lifetime: PhantomData<&'a ()>,
}

/*
TODO: We need to check if the task id in the context of an operation hasn't changed (we would need to change who we are waiting for).
*/

impl<'a> Drop for ExecutorOperation<'a> {
    fn drop(&mut self) {
        if self.must_cancel && !self.done {
            // Must submit a cancelation of the operation and then park the thread
            // until it is cancelled.
            todo!()
        }

        // TODO: We must mininally mark it as dropped so that the executor can
        // clean it up (or if it is already completed, we need to )
    }
}

impl<'a> ExecutorOperation<'a> {
    /// NOTE: The operation must outlive all data referenced in the operation.
    pub fn submit(op: IoUringOp<'a>) -> impl Future<Output = Result<ExecutorOperation<'a>>> {
        ExecutorOperationSubmitFuture { op: Some(op) }
    }

    pub fn wait(self) -> impl Future<Output = Result<IoUringResult>> + 'a {
        ExecutorOperationWaitFuture { op: self }
    }
}

struct ExecutorOperationSubmitFuture<'a> {
    op: Option<IoUringOp<'a>>,
}

impl<'a> ExecutorOperationSubmitFuture<'a> {
    fn poll_with_result(&mut self, context: &mut Context<'_>) -> Result<ExecutorOperation<'a>> {
        let task_entry = retrieve_task_entry(context)
            .ok_or_else(|| err_msg("Not running inside an executor"))?;

        let shared = task_entry.executor_shared.clone();
        let task_id = task_entry.id;

        let mut submissions = shared.io_uring.submissions.lock().unwrap();

        let op_id = submissions.next_operation_id;
        submissions.next_operation_id += 1;

        // The future will only ever get polled once.
        let op = self.op.take().unwrap();

        let must_cancel = op.try_into_static().is_none();

        unsafe {
            submissions.submission_ring.submit(op, op_id)?;
        }

        submissions.operations.insert(
            op_id,
            ExecutorOperationState {
                task_id,
                result: None,
            },
        );

        Ok(ExecutorOperation {
            executor_shared: shared.clone(),
            id: op_id,
            done: false,
            must_cancel,
            lifetime: PhantomData,
        })
    }
}

impl<'a> Future for ExecutorOperationSubmitFuture<'a> {
    type Output = Result<ExecutorOperation<'a>>;

    fn poll(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };

        Poll::Ready(this.poll_with_result(context))
    }
}

struct ExecutorOperationWaitFuture<'a> {
    op: ExecutorOperation<'a>,
}

impl<'a> Future for ExecutorOperationWaitFuture<'a> {
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
