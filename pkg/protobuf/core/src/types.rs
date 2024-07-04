use core::marker::PhantomData;

/// In range [1, 2^29 - 1] except [19000, 19999] is reserved.
pub type FieldNumber = u32;

pub type ExtensionNumberType = FieldNumber;

/// Type used in memory to store the value of an enum field.
/// NOTE: Can be negative.
pub type EnumValue = i32;

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct TypedFieldNumber<T> {
    num: FieldNumber,
    t: PhantomData<T>,
}

impl<T> TypedFieldNumber<T> {
    pub const fn new(num: FieldNumber) -> Self {
        Self {
            num,
            t: PhantomData,
        }
    }

    pub fn raw(&self) -> FieldNumber {
        self.num
    }
}
