use core::borrow::{Borrow, BorrowMut};
use core::ops::{Deref, DerefMut};

use crate::cortex_m::channel::*;
use crate::cortex_m::mutex::*;

pub struct CondValue<T> {
    value: Mutex<T>,
    waiters: Channel<()>,
}

impl<T> CondValue<T> {
    pub fn new(initial_value: T) -> Self {
        Self {
            value: Mutex::new(initial_value),
            waiters: Channel::new(),
        }
    }

    pub async fn lock<'a>(&'a self) -> CondValueGuard<'a, T> {
        CondValueGuard {
            inst: self,
            value: self.value.lock().await,
        }
    }
}

pub struct CondValueGuard<'a, T> {
    inst: &'a CondValue<T>,
    value: MutexGuard<'a, T>,
}

impl<'a, T> Borrow<T> for CondValueGuard<'a, T> {
    fn borrow(&self) -> &T {
        &self.value
    }
}

impl<'a, T> BorrowMut<T> for CondValueGuard<'a, T> {
    fn borrow_mut(&mut self) -> &mut T {
        &mut self.value
    }
}

impl<'a, T> Deref for CondValueGuard<'a, T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.value
    }
}

impl<'a, T> DerefMut for CondValueGuard<'a, T> {
    fn deref_mut(&mut self) -> &mut T {
        &mut self.value
    }
}

impl<'a, T> CondValueGuard<'a, T> {
    pub async fn wait(self) {
        let inst = self.inst;
        drop(self.value);

        let _ = inst.waiters.recv().await;
    }

    pub async fn notify_one(&mut self) {
        let _ = self.inst.waiters.try_send(()).await;
    }
}
