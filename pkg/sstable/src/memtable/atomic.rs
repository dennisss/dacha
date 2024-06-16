use std::sync::atomic::{AtomicPtr, Ordering};
use std::sync::Arc;

/// Equivalent to a Mutex<Option<Arc<T>>> implemented using atomic operation.
pub struct AtomicArc<T> {
    ptr: AtomicPtr<T>,
}

impl<T> Drop for AtomicArc<T> {
    fn drop(&mut self) {
        let ptr = self.ptr.load(Ordering::SeqCst);
        if ptr != core::ptr::null_mut() {
            unsafe { Arc::decrement_strong_count(ptr) };
        }
    }
}

impl<T> Default for AtomicArc<T> {
    fn default() -> Self {
        Self {
            ptr: AtomicPtr::new(core::ptr::null_mut()),
        }
    }
}

impl<T> Clone for AtomicArc<T> {
    fn clone(&self) -> Self {
        /*
        Optimized version of:
            let inst = Self::default();
            inst.store(self.load());
            inst
        */

        let ptr = self.ptr.load(Ordering::SeqCst);
        if ptr != core::ptr::null_mut() {
            unsafe { Arc::increment_strong_count(ptr) }
        }

        Self {
            ptr: AtomicPtr::new(ptr),
        }
    }
}

impl<T> AtomicArc<T> {
    pub fn load(&self) -> Option<Arc<T>> {
        let ptr = self.ptr.load(Ordering::SeqCst);
        if ptr == core::ptr::null_mut() {
            return None;
        }

        unsafe {
            Arc::increment_strong_count(ptr);
            Some(Arc::from_raw(ptr))
        }
    }

    pub fn store(&self, value: Option<Arc<T>>) {
        // TODO: This must a swap.

        let new_ptr = {
            if let Some(value) = value {
                unsafe { core::mem::transmute(Arc::into_raw(value)) }
            } else {
                core::ptr::null_mut()
            }
        };

        let old_ptr = self.ptr.swap(new_ptr, Ordering::SeqCst);
        if old_ptr != core::ptr::null_mut() {
            unsafe { Arc::decrement_strong_count(old_ptr) };
        }
    }
}
