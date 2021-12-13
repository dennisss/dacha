use core::task::{Context, RawWaker, RawWakerVTable, Waker};

unsafe fn raw_waker_clone(data: *const ()) -> RawWaker {
    RAW_WAKER
}

unsafe fn raw_waker_wake(data: *const ()) {}

unsafe fn raw_waker_wake_by_ref(data: *const ()) {}

unsafe fn raw_waker_drop(data: *const ()) {}

const RAW_WAKER_VTABLE: RawWakerVTable = RawWakerVTable::new(
    raw_waker_clone,
    raw_waker_wake,
    raw_waker_wake_by_ref,
    raw_waker_drop,
);

pub const RAW_WAKER: RawWaker = RawWaker::new(1 as *const (), &RAW_WAKER_VTABLE);
