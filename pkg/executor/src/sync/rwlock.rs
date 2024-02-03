use core::ops::{Deref, DerefMut};
use core::time::Duration;

use base_error::Result;
use common::async_std::sync::{
    RwLock, RwLockReadGuard, RwLockUpgradableReadGuard, RwLockWriteGuard,
};

use crate::sync::PoisonError;

type RwLockImpl<T> = common::async_std::sync::RwLock<T>;
type RwLockReadGuardImpl<'a, T> = common::async_std::sync::RwLockReadGuard<'a, T>;
type RwLockUpgradableReadGuardImpl<'a, T> =
    common::async_std::sync::RwLockUpgradableReadGuard<'a, T>;
type RwLockWriteGuardImpl<'a, T> = common::async_std::sync::RwLockWriteGuard<'a, T>;

pub struct AsyncRwLock<T> {
    inner: RwLockImpl<AsyncRwLockValue<T>>,
}

struct AsyncRwLockValue<T> {
    data: T,
    poisoned: bool,
}

impl<T> AsyncRwLock<T> {
    pub fn new(data: T) -> Self {
        Self {
            inner: RwLockImpl::new(AsyncRwLockValue {
                data,
                poisoned: false,
            }),
        }
    }

    pub async fn read<'a>(&'a self) -> Result<AsyncRwLockReadGuard<'a, T>, PoisonError> {
        let guard = self.inner.read().await;
        if guard.poisoned {
            return Err(PoisonError::MutationCancelled);
        }

        Ok(AsyncRwLockReadGuard { inner: guard })
    }

    pub async fn upgradeable_read<'a>(
        &'a self,
    ) -> Result<AsyncRwLockUpgradableReadGuard<'a, T>, PoisonError> {
        let guard = self.inner.upgradable_read().await;
        if guard.poisoned {
            return Err(PoisonError::MutationCancelled);
        }

        Ok(AsyncRwLockUpgradableReadGuard { inner: guard })
    }

    pub async fn write<'a>(&'a self) -> Result<AsyncRwLockWritePermit<'a, T>, PoisonError> {
        let guard = self.inner.write().await;
        if guard.poisoned {
            return Err(PoisonError::MutationCancelled);
        }

        Ok(AsyncRwLockWritePermit { inner: guard })
    }
}

pub struct AsyncRwLockWritePermit<'a, T> {
    inner: RwLockWriteGuardImpl<'a, AsyncRwLockValue<T>>,
}

impl<'a, T> Drop for AsyncRwLockWritePermit<'a, T> {
    fn drop(&mut self) {
        if self.inner.poisoned {
            panic!("Poisoned");
        }
    }
}

impl<'a, T> AsyncRwLockWritePermit<'a, T> {
    pub fn enter(mut self) -> AsyncRwLockWriteGuard<'a, T> {
        self.inner.poisoned = true;
        AsyncRwLockWriteGuard { permit: self }
    }
}

pub struct AsyncRwLockWriteGuard<'a, T> {
    permit: AsyncRwLockWritePermit<'a, T>,
}

impl<'a, T> AsyncRwLockWriteGuard<'a, T> {
    pub fn exit(mut self) {
        self.permit.inner.poisoned = false;
    }
}

impl<'a, T> Deref for AsyncRwLockWriteGuard<'a, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.permit.inner.data
    }
}

impl<'a, T> DerefMut for AsyncRwLockWriteGuard<'a, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.permit.inner.data
    }
}

pub struct AsyncRwLockReadGuard<'a, T> {
    inner: RwLockReadGuardImpl<'a, AsyncRwLockValue<T>>,
}

impl<'a, T> Deref for AsyncRwLockReadGuard<'a, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.inner.data
    }
}

pub struct AsyncRwLockUpgradableReadGuard<'a, T> {
    inner: RwLockUpgradableReadGuardImpl<'a, AsyncRwLockValue<T>>,
}

impl<'a, T> AsyncRwLockUpgradableReadGuard<'a, T> {
    pub async fn upgrade(self) -> AsyncRwLockWritePermit<'a, T> {
        let guard = RwLockUpgradableReadGuardImpl::upgrade(self.inner).await;
        AsyncRwLockWritePermit { inner: guard }
    }
}

impl<'a, T> Deref for AsyncRwLockUpgradableReadGuard<'a, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.inner.data
    }
}
