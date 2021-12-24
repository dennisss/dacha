use alloc::vec::Vec;

use common::const_default::ConstDefault;

/// Used to represent the 'bytes' type in memory.
///
/// TODO: Implement with Cow to make this easy to clone.
#[derive(Clone, PartialEq, Debug, Default)]
pub struct BytesField(pub Vec<u8>);

impl core::convert::From<Vec<u8>> for BytesField {
    fn from(v: Vec<u8>) -> Self {
        Self(v)
    }
}

impl core::convert::From<&[u8]> for BytesField {
    fn from(v: &[u8]) -> Self {
        Self(v.to_vec())
    }
}

impl core::ops::Deref for BytesField {
    type Target = Vec<u8>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl core::ops::DerefMut for BytesField {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl ConstDefault for BytesField {
    const DEFAULT: Self = BytesField(Vec::new());
}
