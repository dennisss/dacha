use crate::avr::interrupts::*;
use core::cell::Cell;

pub struct Mutex {
    locked: Cell<bool>,
}

impl Mutex {
    pub const fn new() -> Self {
        Self {
            locked: Cell::new(false),
        }
    }

    pub async fn lock<'a>(&'a self) -> MutexLock<'a> {
        // TODO: Maybe assert that interrupts are disabled.

        while self.locked.get() {
            InterruptEvent::Internal.to_future().await;
        }

        self.locked.set(true);

        MutexLock { mutex: self }
    }
}

pub struct MutexLock<'a> {
    mutex: &'a Mutex,
}

impl<'a> Drop for MutexLock<'a> {
    fn drop(&mut self) {
        self.mutex.locked.set(false);
        fire_internal_interrupt();
    }
}
