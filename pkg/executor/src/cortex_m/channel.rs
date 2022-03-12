use crate::interrupts::{trigger_pendsv, wait_for_pendsv};
use crate::mutex::*;

/// Container for relying a value from some producer(s) to some consumer(s).
///
/// NOTE: This is currently limited to only being able to queue one value at a
/// time. Senders must wait until a consumer takes the value before being able
/// to send a new value.
pub struct Channel<T> {
    value: Mutex<Option<T>>,
}

impl<T> Channel<T> {
    pub const fn new() -> Self {
        Self {
            value: Mutex::new(None),
        }
    }

    pub async fn try_send(&self, value: T) -> bool {
        let mut value_guard = self.value.lock().await;
        if !value_guard.is_some() {
            *value_guard = Some(value);
            trigger_pendsv();
            true
        } else {
            false
        }
    }

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

    pub async fn try_recv(&self) -> Option<T> {
        let mut value_guard = self.value.lock().await;
        let value = value_guard.take();
        if value.is_some() {
            trigger_pendsv();
        }

        value
    }

    pub async fn recv(&self) -> T {
        loop {
            let mut value_guard = self.value.lock().await;
            if let Some(value) = value_guard.take() {
                return value;
            }

            // TODO: Register a waker first and then release the lock.
            drop(value_guard);
            wait_for_pendsv().await;
        }
    }
}
