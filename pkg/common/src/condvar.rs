#[cfg(feature = "alloc")]
use alloc::string::String;
#[cfg(feature = "alloc")]
use alloc::vec::Vec;
use std::borrow::{Borrow, BorrowMut};
use std::ops::{Deref, DerefMut};

use async_std::sync::{Mutex, MutexGuard};
use futures::channel::oneshot;

// TODO: Simplify the imlpementation of this by just using a regular channel.

/// Pretty much a futures based implementation of a conditional variable that
/// owns the condition value.
/// Unlike a conditional variable, this will not relock the mutex after the wait
/// is done.
///
/// NOTE: It should not be locked for a long period of time as that is still a
/// blocking operation
/// We also allow listeners to store a small value when they call wait()
/// A notifier can optionally read this value to filter exactly which waiters
/// are woken up
pub struct Condvar<V, T = ()> {
    inner: Mutex<CondvarInner<V, T>>,
}

struct CondvarInner<V, T> {
    value: V,
    waiters: Vec<(oneshot::Sender<()>, T)>,
}

impl<V, T> CondvarInner<V, T> {
    /// Garbage collects all waiters which are no longer being waited on
    fn collect(&mut self) {
        let mut i = 0;
        while i < self.waiters.len() {
            let dropped = self.waiters[i].0.is_canceled();

            if dropped {
                self.waiters.swap_remove(i);
            } else {
                i += 1;
            }
        }
    }
}

impl<V, T> Condvar<V, T> {
    // TODO: It would be most reasonable to give the comparator function up
    // front or implement it upfront as a trait upfront so that the notifier
    // doesn't have to worry about passing in a tester
    pub fn new(initial_value: V) -> Self {
        Condvar {
            // TODO: Implement a a lock free list + Atomic variable instead?
            inner: Mutex::new(CondvarInner {
                value: initial_value,
                waiters: vec![],
            }),
        }
    }

    pub async fn lock<'a>(&'a self) -> CondvarGuard<'a, V, T> {
        CondvarGuard {
            guard: self.inner.lock().await,
        }
    }
}

pub struct CondvarGuard<'a, V, T> {
    guard: MutexGuard<'a, CondvarInner<V, T>>,
}

impl<'a, V, T> Borrow<V> for CondvarGuard<'a, V, T> {
    fn borrow(&self) -> &V {
        &self.guard.value
    }
}

impl<'a, V, T> BorrowMut<V> for CondvarGuard<'a, V, T> {
    fn borrow_mut(&mut self) -> &mut V {
        &mut self.guard.value
    }
}

impl<'a, V, T> Deref for CondvarGuard<'a, V, T> {
    type Target = V;
    fn deref(&self) -> &V {
        &self.guard.value
    }
}

impl<'a, V, T> DerefMut for CondvarGuard<'a, V, T> {
    fn deref_mut(&mut self) -> &mut V {
        &mut self.guard.value
    }
}

impl<'a, V, T> CondvarGuard<'a, V, T> {
    pub async fn wait(self, data: T) {
        // LockResult<MutexGuard<'a, T>> {
        let (tx, rx) = oneshot::channel();
        let mut guard = self.guard;

        // TODO: Currently no mechanism for effeciently cleaning up waiters
        // without having to look through all of them
        guard.collect();

        guard.waiters.push((tx, data));

        // NOTE: This will be dropped anyway as soon as the future is returned
        drop(guard);

        rx.await.ok(); // TODO: Check this.
    }

    // TODO: Should we immediately consume and drop the guard
    pub fn notify_filter<F>(&mut self, f: F)
    where
        F: Fn(&T) -> bool,
    {
        let guard = &mut self.guard;

        let mut i = guard.waiters.len();
        while i > 0 {
            let notify = f(&guard.waiters[i - 1].1);
            if notify {
                let (tx, _) = guard.waiters.swap_remove(i - 1);
                if let Err(_) = tx.send(()) {
                    // In this case, the waiter was deallocated and doesn't
                    // matter anymore
                    // TODO: I don't think the oneshot channel emits any real
                    // errors though and should always succeed if not
                    // deallocated?
                }
            }

            i -= 1;
        }
    }

    pub fn notify_all(&mut self) {
        self.notify_filter(|_| true);
    }
}
