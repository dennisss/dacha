use core::ops::{Deref, DerefMut};

use crate::sync::PoisonError;

type MutexImpl<T> = std::sync::Mutex<T>;

type MutexGuardImpl<'a, T> = std::sync::MutexGuard<'a, T>;

/// NOTE: This can not be used on any single threaded systems since it would
/// prevent co-operative preemption as individual futures would continously
/// block if a lock is not available.
pub struct SyncMutex<T> {
    inner: MutexImpl<SyncMutexValue<T>>,
}

struct SyncMutexValue<T> {
    data: T,
    poisoned: bool,
}

impl<T> SyncMutex<T> {
    pub fn new(data: T) -> Self {
        Self {
            inner: MutexImpl::new(SyncMutexValue {
                data,
                poisoned: false,
            }),
        }
    }

    /// NOTE: we do not allow using permits/enter() with sync mutexes as this
    /// makes it harder to guarantee that no async behaviors happen after the
    /// locking. This is important on single threaded non-preempting systems
    /// where lock() can't block.
    pub fn apply<V, F: for<'b> FnOnce(&'b mut T) -> V>(&self, f: F) -> Result<V, PoisonError> {
        let mut guard = self
            .inner
            .lock()
            .map_err(|_| PoisonError::MutationCancelled)?;

        guard.poisoned = true;

        let ret = f(&mut guard.data);

        guard.poisoned = false;

        Ok(ret)
    }
}

impl<T: Clone> SyncMutex<T> {
    pub fn read(&self) -> Result<T, PoisonError> {
        self.apply(|v| v.clone())
    }
}

impl<T: Default> Default for SyncMutex<T> {
    fn default() -> Self {
        Self::new(T::default())
    }
}
