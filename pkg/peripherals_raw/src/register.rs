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

pub trait AddressBlock {
    fn base_address(&self) -> u32;

    fn offset(&self, offset: u32) -> OffsetAddressBlock<Self>
    where
        Self: Sized + Clone,
    {
        OffsetAddressBlock { base: self.clone() }
    }
}

#[derive(Clone, Copy)]
pub struct OffsetAddressBlock<Base: AddressBlock> {
    // offset: u32,
    base: Base,
}

impl<Base: AddressBlock> AddressBlock for OffsetAddressBlock<Base> {
    fn base_address(&self) -> u32 {
        self.base.base_address() + 0
    }
}

pub trait RegisterRead {
    type Value;

    fn read(&self) -> Self::Value;
}

pub trait RegisterWrite {
    type Value;

    fn write(&mut self, value: Self::Value);
}
