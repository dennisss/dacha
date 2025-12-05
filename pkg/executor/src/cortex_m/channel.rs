use crate::interrupts::{trigger_pendsv, wait_for_pendsv};
use crate::critical_mutex::*;
use crate::CriticalSection;

// TODO: For event sending, we can probably just optimize this down further.

/// Container for relaying a value from some producer(s) to some consumer(s).
///
/// NOTE: This is currently limited to only being able to queue one value at a
/// time. Senders must wait until a consumer takes the value before being able
/// to send a new value.
pub struct Channel<T> {
    value: CriticalMutex<Option<T>>,
}

impl<T> Channel<T> {
    pub const fn new() -> Self {
        Self {
            value: CriticalMutex::new(None),
        }
    }

    pub fn try_send(&self, value: T) -> bool {
        let mut value_guard = self.value.lock();
        if !value_guard.is_some() {
            *value_guard = Some(value);
            trigger_pendsv();
            true
        } else {
            false
        }
    }

    /*
    pub async fn send(&self, value: T) {
        loop {
            let mut value_guard = self.value.lock().await;
            if !value_guard.is_some() {
                *value_guard = Some(value);
                trigger_pendsv();
                break;
            }

            // TODO: Register a waker first and then release the lock.
            drop(value_guard);
            wait_for_pendsv().await;
        }
    }
    */

    pub fn try_recv(&self) -> Option<T> {
        let mut value_guard = self.value.lock();
        let value = value_guard.take();
        if value.is_some() {
            trigger_pendsv();
        }

        value
    }

    pub async fn recv(&self) -> T {
        loop {
            // TODO: This is redundant with the critical seciton used for the locking.
            let cs = CriticalSection::new();

            let mut value_guard = self.value.lock();
            if let Some(value) = value_guard.take() {
                return value;
            }

            drop(value_guard);
            wait_for_pendsv(cs).await;
        }
    }
}
