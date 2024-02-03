use crate::{lock, sync::AsyncMutex};

pub struct Singleton<T> {
    value: AsyncMutex<Option<T>>,
}

impl<T: 'static + Sync> Singleton<T> {
    pub const fn uninit() -> Self {
        Self {
            value: AsyncMutex::new(None),
        }
    }

    pub async fn set(&'static self, value: T) -> &'static T {
        lock!(value_guard <= self.value.lock().await.unwrap(), {
            assert!(value_guard.is_none());
            *value_guard = Some(value);

            // We only ever allow setting the value once so
            unsafe { core::mem::transmute::<_, &'static T>(value_guard.as_ref().unwrap()) }
        })
    }
}
