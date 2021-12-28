use core::cell::UnsafeCell;
use core::ptr::{read_volatile, write_volatile};

#[repr(transparent)]
pub struct RawRegister<T> {
    value: UnsafeCell<T>,
}

impl<T: Copy> RawRegister<T> {
    // pub const fn new(value: T) ->

    pub fn read(&self) -> T {
        unsafe { read_volatile(self.value.get()) }
    }

    pub fn write(&mut self, value: T) {
        unsafe { write_volatile(self.value.get(), value) }
    }
}
