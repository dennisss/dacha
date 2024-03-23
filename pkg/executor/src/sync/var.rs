use alloc::string::String;
use alloc::vec::Vec;
use core::borrow::{Borrow, BorrowMut};
use core::future::Future;
use core::ops::{Deref, DerefMut};

use crate::channel::oneshot;
use crate::sync::{
    AsyncMutex, AsyncMutexGuard, AsyncMutexPermit, AsyncMutexReadOnlyGuard, PoisonError,
};

pub struct AsyncVariable<T> {
    inner: AsyncMutex<AsyncVariableInner<T>>,
}

struct AsyncVariableInner<T> {
    value: T,
    waiters: Vec<oneshot::Sender<()>>,
}

impl<T> AsyncVariable<T> {
    pub fn new(initial_value: T) -> Self {
        Self {
            // TODO: Implement a a lock free list + Atomic variable instead?
            inner: AsyncMutex::new(AsyncVariableInner {
                value: initial_value,
                waiters: vec![],
            }),
        }
    }

    pub async fn lock<'a>(&'a self) -> Result<AsyncVariablePermit<'a, T>, PoisonError> {
        Ok(AsyncVariablePermit {
            inner: self.inner.lock().await?,
        })
    }
}

impl<T: Default> Default for AsyncVariable<T> {
    fn default() -> Self {
        Self::new(T::default())
    }
}

impl<T> AsyncVariableInner<T> {
    fn wait(mut guard: AsyncMutexGuard<'_, Self>) -> impl Future<Output = ()> {
        let (tx, rx) = oneshot::channel();

        // TODO: Currently no mechanism for effeciently cleaning up waiters
        // without having to look through all of them
        guard.collect();

        guard.waiters.push(tx);

        guard.exit();

        async move {
            rx.recv().await.ok();
        }
    }

    /// Garbage collects all waiters which are no longer being waited on
    fn collect(&mut self) {
        let mut i = 0;
        while i < self.waiters.len() {
            let dropped = self.waiters[i].is_closed();

            if dropped {
                self.waiters.swap_remove(i);
            } else {
                i += 1;
            }
        }
    }

    fn notify_all(&mut self) {
        for tx in self.waiters.drain(0..) {
            tx.send(()).ok();
        }
    }
}

pub struct AsyncVariablePermit<'a, T> {
    inner: AsyncMutexPermit<'a, AsyncVariableInner<T>>,
}

impl<'a, T> AsyncVariablePermit<'a, T> {
    pub fn enter(self) -> AsyncVariableGuard<'a, T> {
        AsyncVariableGuard {
            inner: self.inner.enter(),
        }
    }

    pub fn read_exclusive(self) -> AsyncVariableReadOnlyGuard<'a, T> {
        AsyncVariableReadOnlyGuard {
            inner: Some(self.inner.enter()),
        }
    }
}

// TODO: If the guard gets poisoned, then notify all waiters.

pub struct AsyncVariableGuard<'a, T> {
    inner: AsyncMutexGuard<'a, AsyncVariableInner<T>>,
}

impl<'a, T> AsyncVariableGuard<'a, T> {
    pub fn notify_all(&mut self) {
        self.inner.notify_all();
    }

    pub fn wait(mut self) -> impl Future<Output = ()> {
        AsyncVariableInner::wait(self.inner)
    }

    pub fn exit(self) {
        self.inner.exit();
    }

    pub unsafe fn downgrade(self) -> AsyncVariableReadOnlyGuard<'a, T> {
        AsyncVariableReadOnlyGuard {
            inner: Some(self.inner),
        }
    }

    pub unsafe fn unpoison(&mut self) {
        self.inner.unpoison();
    }
}

impl<'a, T> Borrow<T> for AsyncVariableGuard<'a, T> {
    fn borrow(&self) -> &T {
        &self.inner.value
    }
}

impl<'a, T> BorrowMut<T> for AsyncVariableGuard<'a, T> {
    fn borrow_mut(&mut self) -> &mut T {
        &mut self.inner.value
    }
}

impl<'a, T> Deref for AsyncVariableGuard<'a, T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.inner.value
    }
}

impl<'a, T> DerefMut for AsyncVariableGuard<'a, T> {
    fn deref_mut(&mut self) -> &mut T {
        &mut self.inner.value
    }
}

pub struct AsyncVariableReadOnlyGuard<'a, T> {
    // NOTE: This can't directly use the AsyncMutexReadOnlyGuard as we still allow mutating the
    // waiters list. The read-only behavior is emulated with the below custom Drop implementation.
    inner: Option<AsyncMutexGuard<'a, AsyncVariableInner<T>>>,
}

impl<'a, T> Drop for AsyncVariableReadOnlyGuard<'a, T> {
    fn drop(&mut self) {
        if let Some(inner) = self.inner.take() {
            inner.exit();
        }
    }
}

impl<'a, T> AsyncVariableReadOnlyGuard<'a, T> {
    pub fn downgrade(mut self) -> AsyncVariablePermit<'a, T> {
        AsyncVariablePermit {
            inner: unsafe { self.inner.take().unwrap().downgrade() }.upgrade(),
        }
    }

    pub fn wait(mut self) -> impl Future<Output = ()> {
        AsyncVariableInner::wait(self.inner.take().unwrap())
    }
}

impl<'a, T> Deref for AsyncVariableReadOnlyGuard<'a, T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.inner.as_ref().unwrap().value
    }
}
