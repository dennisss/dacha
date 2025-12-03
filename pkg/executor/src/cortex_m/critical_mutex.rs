use core::cell::{Cell, UnsafeCell};
use core::ops::Deref;
use core::ops::DerefMut;

use crate::CriticalSection;

/// A mutex for single-core devices that requires that all operations
/// on the guarded value occur syncronously before the guard is dropped.
///
/// The only catch is that this will panic if you attempt to lock if while it
/// is already locked.
///
/// If the user doesn't need the task/thread to be suspendable/cancellable
/// while holding the lock, then we can simplify the locking criteria to simply
/// disabling all interrupts and panic if these is nested locking.
///
/// Note that if threading is purely cooperative, then we could go even further
/// and not disable interrupts at all.
///
/// TODO: Prevent usage on non-single threaded devices.
pub struct CriticalMutex<T> {
    value: UnsafeCell<T>,
    locked: Cell<bool>,
}

impl<T> CriticalMutex<T> {
    pub const fn new(value: T) -> Self {
        Self {
            value: UnsafeCell::new(value),
            locked: Cell::new(false)
        }
    }

    pub fn lock<'a>(&'a self) -> CriticalMutexGuard<'a, T> {
        let cs = CriticalSection::new();
        assert!(!self.locked.get());
        self.locked.set(true);
        CriticalMutexGuard { inst: self, cs }
    }
}

unsafe impl<T: Send> Send for CriticalMutex<T> {}
unsafe impl<T: Send> Sync for CriticalMutex<T> {}

/// NOTE: This should be !Send due to CriticalSection.
pub struct CriticalMutexGuard<'a, T> {
    inst: &'a CriticalMutex<T>,
    cs: CriticalSection
}

impl<'a, T> Drop for CriticalMutexGuard<'a, T> {
    fn drop(&mut self) {
        self.inst.locked.set(false);
    }
}

impl<'a, T> Deref for CriticalMutexGuard<'a, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        unsafe { &*self.inst.value.get() }
    }
}

impl<'a, T> DerefMut for CriticalMutexGuard<'a, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { &mut *self.inst.value.get() }
    }
}
