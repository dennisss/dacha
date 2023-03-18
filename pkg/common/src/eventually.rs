use std::cell::RefCell;

pub struct EventuallyCell<T> {
    value: RefCell<Option<T>>,
}

impl<T> Default for EventuallyCell<T> {
    fn default() -> Self {
        Self {
            value: RefCell::new(None),
        }
    }
}

impl<T> EventuallyCell<T> {
    pub fn set(&self, value: T) {
        let mut v = self.value.borrow_mut();
        assert!(v.is_none());
        *v = Some(value);
    }

    pub fn get<'a>(&'a self) -> &'a T {
        let v = self.value.borrow();
        // Only safe because we only allow setting the value once.
        unsafe { core::mem::transmute(v.as_ref()) }
    }
}
