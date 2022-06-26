use core::ops::{Deref, DerefMut};

/// Pointer to the Some() case of an Option.
///
/// While you have a SomePtr, the Option it points to always contains a Some()
/// case.
pub struct SomePtr<'a, T> {
    value: &'a mut Option<T>,
}

impl<'a, T> SomePtr<'a, T> {
    pub fn get(value: &'a mut Option<T>) -> Option<Self> {
        if value.is_some() {
            Some(Self { value })
        } else {
            None
        }
    }

    pub fn take(self) -> T {
        self.value.take().unwrap()
    }
}

impl<'a, T> Deref for SomePtr<'a, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        unsafe { self.value.as_ref().unwrap_unchecked() }
    }
}

impl<'a, T> DerefMut for SomePtr<'a, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { self.value.as_mut().unwrap_unchecked() }
    }
}
