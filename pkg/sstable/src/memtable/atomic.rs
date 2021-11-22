use std::sync::atomic::AtomicPtr;
use std::sync::Arc;

pub struct AtomicArc<T> {
    ptr: AtomicPtr<T>,
}

impl<T> AtomicArc<T> {
    pub fn new(v: T) -> Self {
        Self {
            ptr: AtomicPtr::new(unsafe { std::mem::transmute(Arc::into_raw(Arc::new(v))) }),
        }
    }
}
