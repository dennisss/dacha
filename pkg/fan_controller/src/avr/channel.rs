use crate::avr::interrupts::*;
use core::cell::UnsafeCell;

/// Used to send data from one thread to another.
///
/// NOTE: This will only queue one value at a time, so senders must block for
/// the receiver to finish processing the data.
///
/// Challenges: Should we be able to
pub struct Channel<T> {
    value: UnsafeCell<Option<T>>,
}

impl<T> Channel<T> {
    pub const fn new() -> Self {
        Self {
            value: UnsafeCell::new(None),
        }
    }

    pub async fn send(&'static self, value: T) {
        let v = unsafe { core::mem::transmute::<*mut Option<T>, &mut Option<T>>(self.value.get()) };
        while v.is_some() {
            InterruptEvent::Internal.to_future().await;
        }

        *v = Some(value);

        fire_internal_interrupt();
    }

    pub async fn recv(&'static self) -> T {
        let v = unsafe { core::mem::transmute::<*mut Option<T>, &mut Option<T>>(self.value.get()) };
        loop {
            if let Some(v) = v.take() {
                fire_internal_interrupt();
                return v;
            }

            InterruptEvent::Internal.to_future().await;
        }
    }
}

unsafe impl<T> Sync for Channel<T> {}
