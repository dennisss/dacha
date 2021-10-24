/// In range [1, 2^29 - 1] except [19000, 19999] is reserved.
pub type FieldNumber = u32;

pub type ExtensionNumberType = FieldNumber;

/// Type used in memory to store the value of an enum field.
/// NOTE: Can be negative.
pub type EnumValue = i32;
