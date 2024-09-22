use std::time::Instant;

/// This is a value which records the time at which it is updated.
#[derive(Clone, Default)]
pub struct TimestampedValue<T> {
    value: Option<(T, Instant)>,
}

impl<T> TimestampedValue<T> {
    pub fn new(value: T, time: Instant) -> Self {
        Self {
            value: Some((value, time)),
        }
    }

    pub fn get(&self) -> Option<&T> {
        self.value.as_ref().map(|(v, _)| v)
    }

    pub fn take(&mut self) -> Self {
        Self {
            value: self.value.take(),
        }
    }

    pub fn insert_if_present(&mut self, value: Self) {
        if value.value.is_some() {
            self.value = value.value;
        }
    }

    pub fn last_updated(&self) -> Option<Instant> {
        self.value.as_ref().map(|(_, t)| t.clone())
    }
}
