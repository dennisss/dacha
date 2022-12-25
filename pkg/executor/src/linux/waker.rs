use core::task::{Context, RawWaker, RawWakerVTable, Waker};
use std::sync::Arc;

use crate::linux::task::TaskEntry;

use super::executor::Executor;

pub(super) fn create_waker(task_entry: Arc<TaskEntry>) -> Waker {
    unsafe {
        Waker::from_raw(RawWaker::new(
            Arc::into_raw(task_entry) as *const (),
            &RAW_WAKER_VTABLE,
        ))
    }
}

/// Looks up the TaskEntry associated with the currently running Task.
pub(super) fn retrieve_task_entry<'a>(context: &'a Context) -> Option<&'a TaskEntry> {
    let raw_waker = context.waker().as_raw();

    if raw_waker.vtable() != &RAW_WAKER_VTABLE {
        return None;
    }

    Some(unsafe { core::mem::transmute(raw_waker.data()) })
}

const RAW_WAKER_VTABLE: RawWakerVTable = RawWakerVTable::new(
    raw_waker_clone,
    raw_waker_wake,
    raw_waker_wake_by_ref,
    raw_waker_drop,
);

unsafe fn raw_waker_clone(data: *const ()) -> RawWaker {
    // TODO: Don't drop this?
    let entry = Arc::from_raw(data as *const TaskEntry);

    let ptr = Arc::into_raw(entry.clone()) as *const ();

    // Keep the original copy alive instead of droping it.
    let _ = Arc::into_raw(entry);

    RawWaker::new(ptr, &RAW_WAKER_VTABLE)
}

unsafe fn raw_waker_wake(data: *const ()) {
    let entry = Arc::from_raw(data as *const TaskEntry);
    Executor::wake_task_entry(&entry, false);
    drop(entry);
}

unsafe fn raw_waker_wake_by_ref(data: *const ()) {
    let entry = Arc::from_raw(data as *const TaskEntry);
    Executor::wake_task_entry(&entry, false);

    // Keep the original copy alive instead of dropping it.
    let _ = Arc::into_raw(entry);
}

unsafe fn raw_waker_drop(data: *const ()) {
    let entry = Arc::from_raw(data as *const TaskEntry);
    drop(entry);
}
