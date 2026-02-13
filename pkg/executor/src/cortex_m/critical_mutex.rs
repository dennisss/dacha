use core::cell::{Cell, UnsafeCell};
use core::ops::Deref;
use core::ops::DerefMut;
use core::intrinsics::unlikely;
use core::marker::PhantomData;

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
pub struct CriticalMutex<T, I = Uninterruptable> {
    value: UnsafeCell<T>,
    locked: Cell<bool>,
    interruption_type: PhantomData<I>
}

impl<T, I: From<CriticalSection>> CriticalMutex<T, I> {
    pub const fn new(value: T) -> Self {
        Self {
            value: UnsafeCell::new(value),
            locked: Cell::new(false),
            interruption_type: PhantomData,
        }
    }

    pub fn lock<'a>(&'a self) -> CriticalMutexGuard<'a, T, I> {
        let cs = CriticalSection::new();

        if unsafe { unlikely(self.locked.get()) } {
            panic!()
        }

        self.locked.set(true);
        CriticalMutexGuard { inst: self, interruption_guard: I::from(cs) }
    }
}

unsafe impl<T: Send, I> Send for CriticalMutex<T, I> {}
unsafe impl<T: Send, I> Sync for CriticalMutex<T, I> {}

/// NOTE: This should be !Send due to CriticalSection.
pub struct CriticalMutexGuard<'a, T, I> {
    inst: &'a CriticalMutex<T, I>,
    interruption_guard: I
}

impl<'a, T, I> Drop for CriticalMutexGuard<'a, T, I> {
    fn drop(&mut self) {
        self.inst.locked.set(false);
    }
}

impl<'a, T, I> Deref for CriticalMutexGuard<'a, T, I> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        unsafe { &*self.inst.value.get() }
    }
}

impl<'a, T, I> DerefMut for CriticalMutexGuard<'a, T, I> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { &mut *self.inst.value.get() }
    }
}

impl<'a, T, I> CriticalMutexGuard<'a, T, I> {
    #[inline(always)]
    pub fn enter(self) -> Self {
        self
    }

    #[inline(always)]
    pub fn exit(self) {
        drop(self);
    }
}


pub struct Interruptable {
    hidden: ()
}

impl From<CriticalSection> for Interruptable {
    fn from(v: CriticalSection) -> Self {
        drop(v);
        Self { hidden: () }
    }
}

pub struct Uninterruptable {
    cs: CriticalSection
}

impl From<CriticalSection> for Uninterruptable {
    fn from(v: CriticalSection) -> Self {
        Self { cs: v }
    }
}
