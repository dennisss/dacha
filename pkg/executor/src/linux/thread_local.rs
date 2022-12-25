use core::marker::PhantomData;
use std::sync::Arc;

use crate::linux::executor::ExecutorShared;
use crate::linux::task::TaskEntry;

#[thread_local]
static mut CURRENT_EXECUTOR: Option<Arc<ExecutorShared>> = None;

#[thread_local]
static mut CURRENT_TASK: Option<Arc<TaskEntry>> = None;

pub(super) struct CurrentExecutorContext<'a> {
    lifetime: PhantomData<&'a ()>,
}

// Must not go to another thread as we rely on thread locals.
impl !Send for CurrentExecutorContext<'_> {}
impl !Sync for CurrentExecutorContext<'_> {}

impl<'a> CurrentExecutorContext<'a> {
    pub fn new(executor_shared: &'a Arc<ExecutorShared>) -> Self {
        // Only safe because we drop it later in the same thread so the destructor will
        // run.
        unsafe {
            assert!(CURRENT_EXECUTOR.is_none());
            CURRENT_EXECUTOR = Some(executor_shared.clone())
        };

        Self {
            lifetime: PhantomData,
        }
    }

    pub fn current() -> Option<Arc<ExecutorShared>> {
        unsafe { CURRENT_EXECUTOR.as_ref().map(|v| v.clone()) }
    }
}

impl<'a> Drop for CurrentExecutorContext<'a> {
    fn drop(&mut self) {
        let inst = unsafe { CURRENT_EXECUTOR.take() };
        drop(inst);
    }
}

// TODO: Deduplicate this with the above one.
pub(super) struct CurrentTaskContext<'a> {
    lifetime: PhantomData<&'a ()>,
}

// Must not go to another thread as we rely on thread locals.
impl !Send for CurrentTaskContext<'_> {}
impl !Sync for CurrentTaskContext<'_> {}

impl<'a> CurrentTaskContext<'a> {
    pub fn new(task_entry: &'a Arc<TaskEntry>) -> Self {
        // Only safe because we drop it later in the same thread so the destructor will
        // run.
        unsafe {
            assert!(CURRENT_TASK.is_none());
            CURRENT_TASK = Some(task_entry.clone())
        };

        Self {
            lifetime: PhantomData,
        }
    }

    pub fn current() -> Option<Arc<TaskEntry>> {
        unsafe { CURRENT_TASK.as_ref().map(|v| v.clone()) }
    }
}

impl<'a> Drop for CurrentTaskContext<'a> {
    fn drop(&mut self) {
        let inst = unsafe { CURRENT_TASK.take() };
        drop(inst);
    }
}
