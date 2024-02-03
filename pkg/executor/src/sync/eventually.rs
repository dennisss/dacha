// TODO: Move out of the linux directory.

use common::errors::*;

use crate::sync::AsyncVariable;

/// A value which is initially null, but will eventually be initially and
/// available.
///
/// This operates using the following constraints:
/// - Once a value is set, is is never changed (can't be replaced and is
///   immutable).
pub struct Eventually<T> {
    value: AsyncVariable<Option<T>>,
}

impl<T> Eventually<T> {
    pub fn new() -> Self {
        Self {
            value: AsyncVariable::new(None),
        }
    }

    /// NOTE: Will fail if the value has already been set.
    pub async fn set(&self, v: T) -> Result<()> {
        let mut value = self.value.lock().await?.enter();
        if value.is_some() {
            value.exit();
            return Err(err_msg("Value already set"));
        }

        *value = Some(v);
        value.notify_all();
        value.exit();
        Ok(())
    }

    pub async fn get<'a>(&'a self) -> &'a T {
        loop {
            let value = self.value.lock().await.unwrap().enter();
            if let Some(v) = value.as_ref() {
                let ret = unsafe { std::mem::transmute(v) };
                value.exit();
                return ret;
            }

            value.wait().await;
        }
    }

    pub async fn has_value(&self) -> bool {
        self.value.lock().await.unwrap().read_exclusive().is_some()
    }
}
