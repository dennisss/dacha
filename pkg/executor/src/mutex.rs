use core::cell::{Cell, UnsafeCell};
use core::ops::Deref;
use core::ops::DerefMut;
use core::sync::atomic::AtomicBool;

use crate::interrupts::{trigger_pendsv, wait_for_pendsv};

pub struct Mutex<T> {
    value: UnsafeCell<T>,
    locked: Cell<bool>,
}

impl<T> Mutex<T> {
    pub const fn new(value: T) -> Self {
        Self {
            value: UnsafeCell::new(value),
            locked: Cell::new(false),
        }
    }

    pub async fn lock<'a>(&'a self) -> MutexGuard<'a, T> {
        while self.locked.get() {
            wait_for_pendsv().await;
        }

        MutexGuard { inst: self }
    }
}

pub struct MutexGuard<'a, T> {
    inst: &'a Mutex<T>,
}

impl<'a, T> Drop for MutexGuard<'a, T> {
    fn drop(&mut self) {
        self.inst.locked.set(false);
        trigger_pendsv();
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
