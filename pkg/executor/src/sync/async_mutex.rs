use core::ops::{Deref, DerefMut};
use core::time::Duration;

use base_error::Result;

#[cfg(feature = "std")]
type MutexImpl<T> = common::async_std::sync::Mutex<T>;

#[cfg(feature = "std")]
type MutexGuardImpl<'a, T> = common::async_std::sync::MutexGuard<'a, T>;

#[cfg(target_label = "cortex_m")]
type MutexImpl<T> = crate::cortex_m::mutex::Mutex<T>;

#[cfg(target_label = "cortex_m")]
type MutexGuardImpl<'a, T> = crate::cortex_m::mutex::MutexGuard<'a, T>;

#[derive(Clone, Copy, Debug, Errable, PartialEq)]
#[cfg_attr(feature = "std", derive(Fail))]
#[repr(u32)]
pub enum PoisonError {
    MutationCancelled,
}

impl core::fmt::Display for PoisonError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{:?}", self)
    }
}

pub struct AsyncMutex<T> {
    inner: MutexImpl<AsyncMutexValue<T>>,
}

struct AsyncMutexValue<T> {
    data: T,
    poisoned: bool,
}

impl<T> AsyncMutex<T> {
    pub const fn new(data: T) -> Self {
        Self {
            inner: MutexImpl::new(AsyncMutexValue {
                data,
                poisoned: false,
            }),
        }
    }

    pub async fn lock<'a>(&'a self) -> Result<AsyncMutexPermit<'a, T>, PoisonError> {
        let guard = self.inner.lock().await;
        if guard.poisoned {
            return Err(PoisonError::MutationCancelled);
        }

        Ok(AsyncMutexPermit { inner: guard })
    }

    pub async fn apply<V, F: for<'b> FnOnce(&'b mut T) -> V>(
        &self,
        f: F,
    ) -> Result<V, PoisonError> {
        Ok(self.lock().await?.apply(f))
    }
}

impl<T: Default> Default for AsyncMutex<T> {
    fn default() -> Self {
        Self::new(T::default())
    }
}

pub struct AsyncMutexPermit<'a, T> {
    inner: MutexGuardImpl<'a, AsyncMutexValue<T>>,
}

impl<'a, T> Drop for AsyncMutexPermit<'a, T> {
    fn drop(&mut self) {
        if self.inner.poisoned {
            panic!("Poisoned");
        }
    }
}

impl<'a, T> AsyncMutexPermit<'a, T> {
    pub fn enter(mut self) -> AsyncMutexGuard<'a, T> {
        self.inner.poisoned = true;
        AsyncMutexGuard { permit: self }
    }

    pub fn apply<V, F: for<'b> FnOnce(&'b mut T) -> V>(mut self, f: F) -> V {
        self.enter().apply(f)
    }

    /// WARNING:
    pub fn read_exclusive(self) -> AsyncMutexReadOnlyGuard<'a, T> {
        AsyncMutexReadOnlyGuard { permit: self }
    }
}

pub struct AsyncMutexGuard<'a, T> {
    permit: AsyncMutexPermit<'a, T>,
}

impl<'a, T> AsyncMutexGuard<'a, T> {
    pub fn apply<V, F: for<'b> FnOnce(&'b mut T) -> V>(mut self, f: F) -> V {
        let v = f(&mut self.permit.inner.data);
        self.exit();
        v
    }

    pub fn exit(mut self) {
        self.permit.inner.poisoned = false;
    }

    pub unsafe fn downgrade(mut self) -> AsyncMutexReadOnlyGuard<'a, T> {
        self.permit.inner.poisoned = true;
        AsyncMutexReadOnlyGuard {
            permit: self.permit,
        }
    }

    pub unsafe fn unpoison(&mut self) {
        self.permit.inner.poisoned = false;
    }
}

impl<'a, T> Deref for AsyncMutexGuard<'a, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.permit.inner.data
    }
}

impl<'a, T> DerefMut for AsyncMutexGuard<'a, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.permit.inner.data
    }
}

pub struct AsyncMutexReadOnlyGuard<'a, T> {
    permit: AsyncMutexPermit<'a, T>,
}

impl<'a, T> AsyncMutexReadOnlyGuard<'a, T> {
    pub fn upgrade(self) -> AsyncMutexPermit<'a, T> {
        self.permit
    }
}

impl<'a, T> Deref for AsyncMutexReadOnlyGuard<'a, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.permit.inner.data
    }
}
