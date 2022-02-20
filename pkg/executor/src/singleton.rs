use crate::mutex::Mutex;

pub struct Singleton<T> {
    value: Mutex<Option<T>>,
}

impl<T: 'static + Sync> Singleton<T> {
    pub const fn uninit() -> Self {
        Self {
            value: Mutex::new(None),
        }
    }

    pub async fn set(&'static self, value: T) -> &'static T {
        let mut value_guard = self.value.lock().await;
        assert!(value_guard.is_none());
        *value_guard = Some(value);

        // We only ever allow setting the value once so
        unsafe { core::mem::transmute::<_, &'static T>(value_guard.as_ref().unwrap()) }
    }
}
