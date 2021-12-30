use core::cell::{Cell, UnsafeCell};
use core::ops::Deref;
use core::ops::DerefMut;
use core::sync::atomic::AtomicBool;

use crate::interrupts::{trigger_pendsv, wait_for_pendsv};

pub struct Mutex<T> {
    value: UnsafeCell<T>,
    locked: Cell<MutexLockState>,
}

#[derive(Clone, Copy, PartialEq)]
enum MutexLockState {
    Unlocked,
    Locked,
    LockedWithWaiters,
}

impl<T> Mutex<T> {
    pub const fn new(value: T) -> Self {
        Self {
            value: UnsafeCell::new(value),
            locked: Cell::new(MutexLockState::Unlocked),
        }
    }

    pub async fn lock<'a>(&'a self) -> MutexGuard<'a, T> {
        loop {
            match self.locked.get() {
                MutexLockState::Unlocked => {
                    self.locked.set(MutexLockState::Locked);
                    break;
                }
                MutexLockState::Locked => {
                    self.locked.set(MutexLockState::LockedWithWaiters);
                }
                MutexLockState::LockedWithWaiters => {}
            }

            wait_for_pendsv().await;
        }

        MutexGuard { inst: self }
    }
}

unsafe impl<T: Send> Send for Mutex<T> {}
unsafe impl<T: Send> Sync for Mutex<T> {}

pub struct MutexGuard<'a, T> {
    inst: &'a Mutex<T>,
}

impl<'a, T> Drop for MutexGuard<'a, T> {
    fn drop(&mut self) {
        let old_state = self.inst.locked.get();
        self.inst.locked.set(MutexLockState::Unlocked);
        if old_state == MutexLockState::LockedWithWaiters {
            trigger_pendsv();
        }
    }
}

impl<'a, T> Deref for MutexGuard<'a, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        unsafe { &*self.inst.value.get() }
    }
}

impl<'a, T> DerefMut for MutexGuard<'a, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { &mut *self.inst.value.get() }
    }
}
