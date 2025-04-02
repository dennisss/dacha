use core::marker::PhantomData;

/// In range [1, 2^29 - 1] except [19000, 19999] is reserved.
///
/// TODO: Need to validate all parsed field numbers in .proto files are (not) in
/// the above rnages.
pub type FieldNumber = u32;

pub type ExtensionNumberType = FieldNumber;

/// Type used in memory to store the value of an enum field.
/// NOTE: Can be negative.
pub type EnumValue = i32;

/// Largest possible field number.
pub const MAX_FIELD_NUMBER: FieldNumber = 536870911;

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

    pub const fn raw(&self) -> FieldNumber {
        self.num
    }
}
