use crate::condvar::Condvar;
use crate::errors::*;

/// A value which is initially null, but will eventually be initially and
/// available.
///
/// This operates using the following constraints:
/// - Once a value is set, is is never changed (can't be replaced and is
///   immutable).
pub struct Eventually<T> {
    value: Condvar<Option<T>>,
}

impl<T> Eventually<T> {
    pub fn new() -> Self {
        Self {
            value: Condvar::new(None),
        }
    }

    /// NOTE: Will fail if the value has already been set.
    pub async fn set(&self, v: T) -> Result<()> {
        let mut value = self.value.lock().await;
        if value.is_some() {
            return Err(err_msg("Value already set"));
        }

        *value = Some(v);
        value.notify_all();
        Ok(())
    }

    pub async fn get<'a>(&'a self) -> &'a T {
        loop {
            let value = self.value.lock().await;
            if let Some(v) = value.as_ref() {
                return unsafe { std::mem::transmute(v) };
            }

            value.wait(()).await;
        }
    }
}
